use std::path::PathBuf;
use std::sync::atomic::Ordering;

use lsp_types::*;
use rsx_diagnostics::semantic_diagnostics;

use crate::index::WorkspaceIndex;
use crate::project::ProjectInfo;
use crate::ra::EmbeddedAnalyzer;

use super::{AnalyzerState, Backend};

impl Backend {
    pub(crate) async fn reparse_and_diagnose(&self, uri: Uri, text: String) -> Vec<Diagnostic> {
        let revision = self.revision.fetch_add(1, Ordering::Relaxed) + 1;
        let file_path = crate::uri::to_path(&uri);
        // Hold the store lock only for the parse + native diagnostics + build-file sync, then release it before the (slower) rust-analyzer query so concurrent completion/hover reads aren't blocked.
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
            let theme_view = project.as_ref().map(ProjectInfo::theme_view);
            let catalog_view = project.as_ref().and_then(ProjectInfo::catalog_view);
            let semantic =
                semantic_diagnostics(&parsed.document, theme_view.as_ref(), catalog_view.as_ref());
            // Mirror the live buffer to its generated `.rs` so the workspace rust-analyzer analyzes the in-flight text — this is what makes completion/hover/definition live instead of one `cargo check` behind. Same output as the `app!` macro produces at compile time.
            let theme = project.as_ref().and_then(|p| p.theme_type.clone());
            if let Some(rsx_path) = file_path.as_deref() {
                crate::build_sync::sync_build_file(rsx_path, &parsed.source, theme.as_deref());
            }
            (semantic, parsed.source.clone(), theme)
        };

        // Keep the workspace `.rsx` index current with the live buffer so `workspace/symbol` and component references see in-flight edits (no-op until the index is first built).
        if let Some(rsx_path) = file_path.as_deref() {
            self.update_index_file(rsx_path.to_path_buf(), source.clone());
        }

        let native: Vec<Diagnostic> = semantic.into_iter().map(Into::into).collect();
        // Overlay the generated Rust into the embedded analyzer and re-publish native+rust merged from a detached task: `full_diagnostics` can be slow, and notifications are awaited in order on the read loop (see server.rs), so blocking here would stall completion. Native diagnostics are returned now for the immediate publish; the task republishes when the analyzer is ready, and skips (leaving native-only published) while it is still loading.
        if let Some(rsx_path) = file_path {
            self.spawn_rust_diagnostics(uri, rsx_path, source, theme, native.clone(), revision);
        }
        native
    }

    /// Off-loop overlay-and-merge of rust-analyzer diagnostics: maps each back onto the `.rsx` via the line map (dropping generated lines with no `.rsx` origin) and republishes native+rust. A staleness guard skips the publish if the buffer changed meanwhile, so out-of-order task completions never resurrect diagnostics for an older revision.
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
            ..
        }) = crate::build_sync::generated_target(&rsx_path, &source, theme.as_deref())
        else {
            return;
        };
        let Some(root) = crate::build_sync::crate_root(&rsx_path) else {
            return;
        };
        self.ensure_loading(root, Some(gen_path.clone()));

        let analyzer = self.analyzer.clone();
        let outgoing = self.outgoing.clone();
        let log = self.outgoing.clone();
        let store = self.store.clone();
        let revisions = self.revision.clone();
        tokio::spawn(async move {
            let raw = tokio::task::spawn_blocking(move || {
                // A newer edit already superseded this one: skip the expensive query without even contending for the analyzer lock (its `full_diagnostics` would be wasted work).
                if revisions.load(Ordering::Relaxed) != revision {
                    return None;
                }
                let lock_at = std::time::Instant::now();
                let mut state = analyzer.lock().ok()?;
                let lock_ms = lock_at.elapsed().as_millis();
                let AnalyzerState::Ready(a) = &mut *state else {
                    return None;
                };
                if !a.knows_file(&gen_path) {
                    *state = AnalyzerState::Idle;
                    return None;
                }
                let ra_at = std::time::Instant::now();
                let result = a.diagnostics(&gen_path, gen_text);
                let ra_ms = ra_at.elapsed().as_millis();
                if lock_ms > 1000 || ra_ms > 1000 {
                    log.log_message(
                        MessageType::INFO,
                        format!("rsx-analyzer: slow diagnostics — lock {lock_ms}ms, ra {ra_ms}ms"),
                    );
                }
                Some(result)
            })
            .await
            .ok()
            .flatten();
            // Analyzer not ready (or graph stale) → leave the native-only diagnostics already published.
            let Some(raw) = raw else {
                return;
            };
            // The buffer moved on while the query ran → a newer revision's task will publish; don't overwrite it with stale diagnostics.
            if store.read().await.latest_source(&uri) != Some(&source) {
                return;
            }

            let mut merged = native;
            merged.extend(raw.into_iter().filter_map(|mut diag| {
                let rsx_line = (*map.get(diag.range.start.line as usize)?)?;
                // The line map is line-granular, so highlight the whole `.rsx` line.
                diag.range = Range {
                    start: Position {
                        line: rsx_line,
                        character: 0,
                    },
                    end: Position {
                        line: rsx_line,
                        character: u32::MAX,
                    },
                };
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

    /// Starts the (slow) workspace load on a blocking thread if it hasn't started yet. Returns immediately; queries that arrive while loading simply yield nothing.
    pub(crate) fn ensure_loading(&self, root: PathBuf, warm: Option<PathBuf>) {
        // `try_lock`, never `lock`: this runs on the single-threaded runtime, and a blocking RA query can hold the mutex for the length of its `analysis` call. Blocking here would stall the whole LSP read loop. Contention means the analyzer is already `Ready`/`Loading` (no query runs while `Idle`), so there is nothing to start — and any state that just reset to `Idle` is picked up by the next edit's call.
        let Ok(mut state) = self.analyzer.try_lock() else {
            return;
        };
        if matches!(*state, AnalyzerState::Idle) {
            *state = AnalyzerState::Loading;
            let analyzer = self.analyzer.clone();
            let outgoing = self.outgoing.clone();
            tokio::task::spawn_blocking(move || {
                outgoing.log_message(
                    MessageType::INFO,
                    format!("rsx-analyzer: loading workspace at {}…", root.display()),
                );
                let started = std::time::Instant::now();
                let loaded = EmbeddedAnalyzer::load(&root);
                let load_ms = started.elapsed().as_millis();
                // Compute the next state (including the slow `warm`) OUTSIDE the state lock, so queries arriving mid-warm just see `Loading` instead of blocking on the mutex for ~15s.
                let new_state = match loaded {
                    Ok(a) => {
                        let warm_ms = if let Some(p) = &warm {
                            let w = std::time::Instant::now();
                            a.warm(p);
                            w.elapsed().as_millis()
                        } else {
                            0
                        };
                        outgoing.log_message(
                            MessageType::INFO,
                            format!(
                                "rsx-analyzer: workspace ready in {load_ms}ms (+{warm_ms}ms warm)"
                            ),
                        );
                        AnalyzerState::Ready(a)
                    }
                    Err(e) => {
                        outgoing.log_message(
                            MessageType::ERROR,
                            format!("rsx-analyzer: workspace load failed: {e:#}"),
                        );
                        AnalyzerState::Failed
                    }
                };
                if let Ok(mut state) = analyzer.lock() {
                    *state = new_state;
                }
            });
        }
    }
}
