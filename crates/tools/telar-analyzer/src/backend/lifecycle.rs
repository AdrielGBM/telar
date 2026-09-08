//! Document lifecycle: reparsing on every edit, mirroring the generated Rust, and publishing diagnostics. Also the one place rust-analyzer is booted.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use lsp_types::*;
use serde_json::Value;
use telar_diagnostics::semantic_diagnostics;

use crate::index::WorkspaceIndex;
use crate::project::ProjectInfo;
use crate::ra::Analyzer;

use super::{AnalyzerHandle, AnalyzerState, Backend};

impl AnalyzerHandle {
    /// The workspace's rust-analyzer, booting it on first use. `None` while a boot another query started is still in flight, or after one failed — rust-analyzer keeps its own workspace up to date once running, so there is nothing here to invalidate or reload.
    pub(crate) async fn get(&self, root: PathBuf) -> Option<Arc<Analyzer>> {
        {
            let mut state = self.state.lock().ok()?;
            match &*state {
                AnalyzerState::Ready(analyzer) => return Some(analyzer.clone()),
                AnalyzerState::Starting | AnalyzerState::Failed => return None,
                AnalyzerState::Idle => *state = AnalyzerState::Starting,
            }
        }

        let options = self
            .options
            .lock()
            .map(|options| options.clone())
            .unwrap_or(Value::Null);
        let outgoing = self.outgoing.clone();
        // Off the runtime thread: the handshake waits on another thread's reply, and workspace discovery walks the filesystem.
        let started =
            tokio::task::spawn_blocking(move || Analyzer::start(&root, options, outgoing))
                .await
                .ok()?;

        let mut state = self.state.lock().ok()?;
        match started {
            Ok(analyzer) => {
                let analyzer = Arc::new(analyzer);
                *state = AnalyzerState::Ready(analyzer.clone());
                Some(analyzer)
            }
            Err(e) => {
                *state = AnalyzerState::Failed;
                self.outgoing.log_message(
                    MessageType::ERROR,
                    format!("telar-analyzer: rust-analyzer failed to start: {e:#}"),
                );
                None
            }
        }
    }

    /// The running analyzer, without booting one. The passthrough uses this: an editor request for a `.rs` file cannot sit waiting on a workspace load, and `initialize` has already started the boot.
    pub(crate) fn ready(&self) -> Option<Arc<Analyzer>> {
        match &*self.state.lock().ok()? {
            AnalyzerState::Ready(analyzer) => Some(analyzer.clone()),
            _ => None,
        }
    }
    /// Records the editor's `initializationOptions` for the session we boot. Written once from `initialize`, before any query can ask for the analyzer.
    pub(crate) fn set_options(&self, options: Value) {
        if let Ok(mut slot) = self.options.lock() {
            *slot = options;
        }
    }

    /// Drops rust-analyzer, and with it the proc-macro server it owns, while the process is still alive. `server.rs` cannot rely on `drop(backend)` for this: spawned request handlers hold their own `Arc<Backend>` clones, so the `Drop` that closes the connection may not run before the process exits.
    pub(crate) fn release(&self) {
        if let Ok(mut state) = self.state.lock() {
            *state = AnalyzerState::Idle;
        }
    }
}

impl Backend {
    pub(crate) async fn reparse_and_diagnose(&self, uri: Uri, text: String) -> Vec<Diagnostic> {
        let revision = self.revision.fetch_add(1, Ordering::Relaxed) + 1;
        let file_path = crate::uri::to_path(&uri);
        // Held only for the parse and native diagnostics, then released before the slower rust-analyzer query, so concurrent completion reads are not blocked.
        let (semantic, source, theme) = {
            let mut store = self.store.write().await;
            let parse_diagnostics = store.reparse(uri.clone(), text);
            if !parse_diagnostics.is_empty() {
                return parse_diagnostics.into_iter().map(Into::into).collect();
            }
            let Some(parsed) = store.get(&uri) else {
                return Vec::new();
            };
            let project = file_path.as_deref().and_then(ProjectInfo::discover);
            let catalog_view = project.as_ref().and_then(ProjectInfo::catalog_view);
            let semantic = semantic_diagnostics(&parsed.document, catalog_view.as_ref());
            // A new `.rsx` needs something on disk for the crate graph to load; the live buffer itself travels as an overlay, never through the build directory.
            let theme = project.as_ref().and_then(|p| p.theme_type.clone());
            if let Some(rsx_path) = file_path.as_deref() {
                crate::build_sync::sync_build_file(
                    rsx_path,
                    &parsed.source,
                    &parsed.document,
                    theme.as_deref(),
                );
            }
            (semantic, parsed.source.clone(), theme)
        };

        // So `workspace/symbol` and component references see in-flight edits. A no-op until the index is built.
        if let Some(rsx_path) = file_path.as_deref() {
            self.update_index_file(rsx_path.to_path_buf(), source.clone());
        }

        let native: Vec<Diagnostic> = semantic.into_iter().map(Into::into).collect();
        // From a detached task, because the rust-analyzer round-trip can be slow and notifications are awaited in order on the read loop. Native diagnostics publish immediately; the task republishes once rust-analyzer answers.
        if let Some(rsx_path) = file_path {
            self.spawn_rust_diagnostics(uri, rsx_path, source, theme, native.clone(), revision);
        }
        native
    }

    /// Off-loop merge of rust-analyzer's diagnostics: maps each back onto the `.rsx` via the line map (dropping generated lines with no `.rsx` origin) and republishes native+rust. A staleness guard skips the publish if the buffer changed meanwhile, so out-of-order task completions never resurrect diagnostics for an older revision.
    fn spawn_rust_diagnostics(
        &self,
        uri: Uri,
        rsx_path: PathBuf,
        source: String,
        theme: Option<String>,
        native: Vec<Diagnostic>,
        revision: u64,
    ) {
        let Some(crate::build_sync::GeneratedTarget {
            path: gen_path,
            code: gen_text,
            map,
        }) = crate::build_sync::generated_target(&rsx_path, &source, theme.as_deref())
        else {
            return;
        };
        let Some(root) = crate::build_sync::crate_root(&rsx_path) else {
            return;
        };

        let analyzer = self.analyzer.clone();
        let outgoing = self.outgoing.clone();
        let store = self.store.clone();
        let revisions = self.revision.clone();
        tokio::spawn(async move {
            // A newer edit already superseded this one, so skip the round-trip entirely.
            if revisions.load(Ordering::Relaxed) != revision {
                return;
            }
            let Some(analyzer) = analyzer.get(root).await else {
                return;
            };
            let raw = analyzer.diagnostics(&gen_path, &gen_text).await;
            // The buffer moved on while the query ran, so a newer revision's task will publish.
            if store.read().await.latest_source(&uri) != Some(&source) {
                return;
            }

            let mut merged = native;
            merged.extend(raw.into_iter().filter_map(|mut diag| {
                diag.range =
                    super::mapping::diagnostic_range(diag.range, &gen_text, &map, &source)?;
                Some(diag)
            }));
            outgoing.publish_diagnostics(uri, merged);
        });
    }

    /// Runs `f` against the workspace `.rsx` index, building it (a one-time disk scan) if it is absent or rooted elsewhere. The scan + query run on a blocking thread so the read loop isn't stalled.
    pub(crate) async fn with_index<T, F>(&self, root: PathBuf, f: F) -> Option<T>
    where
        F: FnOnce(&WorkspaceIndex) -> T + Send + 'static,
        T: Send + 'static,
    {
        let index = self.index.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = index.lock().ok()?;
            if guard.as_ref().map(|i| i.root() != root).unwrap_or(true) {
                *guard = Some(WorkspaceIndex::build(&root));
            }
            Some(f(guard.as_ref().unwrap()))
        })
        .await
        .ok()
        .flatten()
    }

    /// Refreshes a single file in the index from the live buffer after an edit. A no-op until the index has been built (the first query scans disk, picking up everything saved by then).
    fn update_index_file(&self, path: PathBuf, source: String) {
        let index = self.index.clone();
        tokio::task::spawn_blocking(move || {
            if let Ok(mut guard) = index.lock()
                && let Some(idx) = guard.as_mut()
            {
                idx.update(&path, &source);
            }
        });
    }

    pub(crate) async fn analyzer(&self, root: PathBuf) -> Option<Arc<Analyzer>> {
        self.analyzer.get(root).await
    }
}
