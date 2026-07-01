use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use lsp_types::*;
use ra_ap_ide::TextSize;
use rsx_diagnostics::semantic_diagnostics;
use serde_json::json;
use tokio::sync::RwLock;

use crate::analysis::completions::{
    CompletionKind, attribute_key_items, color_items, completion_context, element_name_items,
    signal_items, style_class_items,
};
use crate::analysis::definition::goto_definition;
use crate::analysis::hover::hover_info;
use crate::index::WorkspaceIndex;
use crate::position::{Section, find_section_at};
use crate::project::ProjectInfo;
use crate::ra::{DefinitionTarget, EmbeddedAnalyzer, RefTarget};
use crate::rpc::OutgoingSender;
use crate::store::Store;
use crate::text::{byte_offset, ident_at, name_range, offset_to_position};
use rsx_transpiler::ExprSpan;
use rsx_transpiler::naming::{to_pascal_case, to_snake_case};

/// Lifecycle of the embedded rust-analyzer: loaded lazily on the first `[logic]` query because `load()` is slow (cargo metadata + crate graph).
// Always lives behind `Arc<Mutex<…>>` and is only ever written in place, so the large `Ready` variant is never moved by value — the size disparity clippy flags is irrelevant here.
#[allow(clippy::large_enum_variant)]
enum AnalyzerState {
    Idle,
    Loading,
    Ready(EmbeddedAnalyzer),
    Failed,
}

/// Server-side docs for one batch of rust-analyzer completion items, keyed by generation. The wire items are sent without documentation (lean list); `completionItem/resolve` re-attaches it from here. A new completion batch bumps `generation`, invalidating the previous batch's `data` references.
#[derive(Default)]
struct CompletionCache {
    generation: u64,
    docs: Vec<Option<Documentation>>,
}

pub struct Backend {
    outgoing: OutgoingSender,
    store: Arc<RwLock<Store>>,
    analyzer: Arc<Mutex<AnalyzerState>>,
    // Persistent `.rsx` symbol index (components, `@classes`, component tag usages) backing `workspace/symbol` and cross-file component references/rename. Built lazily on the first query, refreshed per-file on edits and watched-file events. `None` until the first query builds it.
    index: Arc<Mutex<Option<WorkspaceIndex>>>,
    // Deferred documentation for the last rust-analyzer completion batch (see [`CompletionCache`]).
    completion_cache: Arc<Mutex<CompletionCache>>,
    // Monotonic edit counter, bumped on every reparse. A spawned diagnostics task captures the value it was queued for and bails before the expensive rust-analyzer query if a newer edit superseded it, so keystroke-rate edits don't pile up redundant `full_diagnostics` runs behind the lock.
    revision: Arc<AtomicU64>,
}

impl Backend {
    pub fn new(outgoing: OutgoingSender) -> Self {
        Self {
            outgoing,
            store: Arc::new(RwLock::new(Store::new())),
            analyzer: Arc::new(Mutex::new(AnalyzerState::Idle)),
            index: Arc::new(Mutex::new(None)),
            completion_cache: Arc::new(Mutex::new(CompletionCache::default())),
            revision: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn outgoing(&self) -> &OutgoingSender {
        &self.outgoing
    }

    async fn reparse_and_diagnose(&self, uri: Uri, text: String) -> Vec<Diagnostic> {
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
            let semantic = semantic_diagnostics(&parsed.document, theme_view.as_ref());
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

    pub fn initialize(&self) -> InitializeResult {
        InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        "@".to_string(),
                        "$".to_string(),
                        ".".to_string(),
                        ":".to_string(),
                        " ".to_string(),
                        "\"".to_string(),
                    ]),
                    // Docs are deferred to `completionItem/resolve` so the list stays lean on the wire.
                    resolve_provider: Some(true),
                    ..Default::default()
                }),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
                    retrigger_characters: None,
                    work_done_progress_options: Default::default(),
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                document_range_formatting_provider: Some(OneOf::Left(true)),
                selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),
                color_provider: Some(ColorProviderCapability::Simple(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                code_lens_provider: Some(CodeLensOptions {
                    resolve_provider: Some(false),
                }),
                document_link_provider: Some(DocumentLinkOptions {
                    resolve_provider: Some(false),
                    work_done_progress_options: Default::default(),
                }),
                inlay_hint_provider: Some(OneOf::Left(true)),
                document_highlight_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: Default::default(),
                })),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: SemanticTokensLegend {
                                token_types: crate::analysis::semantic_tokens::token_types(),
                                token_modifiers: vec![],
                            },
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            range: Some(false),
                            work_done_progress_options: Default::default(),
                        },
                    ),
                ),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    pub fn initialized(&self) {
        self.outgoing
            .log_message(MessageType::INFO, "rsx-analyzer initialized");
    }

    pub async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let text = params.text_document.text.clone();
        let diagnostics = self.reparse_and_diagnose(uri.clone(), text).await;
        self.outgoing.publish_diagnostics(uri.clone(), diagnostics);
        // Warm the embedded analyzer as soon as a `.rsx` opens, so the slow workspace load overlaps with reading the file instead of stalling the first completion.
        if let Some(rsx_path) = crate::uri::to_path(&uri)
            && let Some(root) = crate::build_sync::crate_root(&rsx_path)
        {
            self.ensure_loading(root, crate::build_sync::generated_path(&rsx_path));
        }
    }

    pub async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        if let Some(change) = params.content_changes.into_iter().last() {
            let diagnostics = self.reparse_and_diagnose(uri.clone(), change.text).await;
            self.outgoing.publish_diagnostics(uri, diagnostics);
        }
    }

    pub async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        self.store.write().await.close(&uri);
    }

    /// `workspace/didChangeWatchedFiles`: the LSP only receives `didChange` for `.rsx`, so edits to hand-written `.rs` files (and `Cargo.toml`/`Cargo.lock`) would otherwise leave the embedded analyzer frozen at load time — breaking go-to-def / diagnostics / repeated renames that cross into real Rust. Refresh each changed `.rs` from disk; a manifest/lockfile change or a created/deleted file invalidates the crate graph, so drop to Idle for a full reload on the next query. A watched `.rsx` event (a sibling file edited/created/deleted outside the editor) also refreshes the workspace symbol index.
    pub async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        let mut to_refresh: Vec<PathBuf> = Vec::new();
        let mut rsx_changes: Vec<(PathBuf, bool)> = Vec::new();
        let mut needs_reload = false;
        for change in params.changes {
            let Some(path) = crate::uri::to_path(&change.uri) else {
                continue;
            };
            // Generated build files are driven by the overlay path; ignore their constant disk churn.
            if crate::build_sync::is_generated_build_file(&path) {
                continue;
            }
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let ext = path.extension().and_then(|e| e.to_str());
            if ext == Some("rsx") {
                // Index maintenance only — `.rsx` modules are overlaid live, never read by the crate graph, so they don't force an analyzer reload.
                rsx_changes.push((path, change.typ == FileChangeType::DELETED));
                continue;
            }
            if name == "Cargo.toml" || name == "Cargo.lock" || change.typ != FileChangeType::CHANGED
            {
                // A manifest/lockfile edit, or a created/deleted file, changes the crate graph → full reload.
                needs_reload = true;
            } else if ext == Some("rs") {
                to_refresh.push(path);
            }
        }

        if !rsx_changes.is_empty() {
            let index = self.index.clone();
            tokio::task::spawn_blocking(move || {
                if let Ok(mut guard) = index.lock()
                    && let Some(idx) = guard.as_mut()
                {
                    for (path, deleted) in rsx_changes {
                        if deleted {
                            idx.remove(&path);
                        } else {
                            idx.refresh_from_disk(&path);
                        }
                    }
                }
            });
        }

        if !needs_reload && to_refresh.is_empty() {
            return;
        }
        let analyzer = self.analyzer.clone();
        let outgoing = self.outgoing.clone();
        // Off the read loop: locking the analyzer can contend with an in-flight RA query.
        tokio::task::spawn_blocking(move || {
            let Ok(mut state) = analyzer.lock() else {
                return;
            };
            if needs_reload {
                // Only disturb a settled analyzer; a load already in flight is re-validated by the next query's `knows_file` check.
                if matches!(*state, AnalyzerState::Ready(_) | AnalyzerState::Failed) {
                    *state = AnalyzerState::Idle;
                    outgoing.log_message(
                        MessageType::INFO,
                        "rsx-analyzer: manifest/file change — workspace will reload on next query"
                            .to_string(),
                    );
                }
                return;
            }
            if let AnalyzerState::Ready(a) = &mut *state {
                for path in &to_refresh {
                    if !a.refresh_from_disk(path) {
                        // A `.rs` the loaded graph doesn't know (e.g. newly created) → reload to pick it up.
                        *state = AnalyzerState::Idle;
                        break;
                    }
                }
            }
        });
    }

    /// Runs `f` against the workspace `.rsx` index, building it (a one-time disk scan) if it is absent or rooted elsewhere. The scan + query run on a blocking thread so the read loop isn't stalled.
    async fn with_index<T, F>(&self, root: PathBuf, f: F) -> Option<T>
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

    pub async fn completion(&self, params: CompletionParams) -> Option<CompletionResponse> {
        let uri = &params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let file_path = crate::uri::to_path(uri);

        let (source, native, theme) = {
            let store = self.store.read().await;
            let parsed = store.get(uri)?;
            let project = file_path.as_deref().and_then(ProjectInfo::discover);
            let native =
                completion_context(&parsed.source, pos.line, pos.character).map(
                    |kind| match kind {
                        CompletionKind::ElementName => {
                            element_name_items(file_path.as_deref().and_then(|p| p.parent()))
                        }
                        CompletionKind::AttributeKey(tag) => attribute_key_items(&tag),
                        CompletionKind::ColorValue => {
                            color_items(&parsed.document, project.as_ref())
                        }
                        CompletionKind::StyleClass => style_class_items(&parsed.document),
                        CompletionKind::SignalRef => signal_items(&parsed.source),
                    },
                );
            let theme = project.as_ref().and_then(|p| p.theme_type.clone());
            (parsed.source.clone(), native, theme)
        };

        if let Some(items) = native {
            return Some(CompletionResponse::Array(items));
        }
        // Outside a native `.rsx` zone: delegate Rust completion to the embedded rust-analyzer over the generated module — line-mapped for `[logic]`, expression-span-mapped for `[view]`.
        let rsx_path = file_path?;
        let items = match find_section_at(&source, pos.line) {
            Section::Logic => {
                self.logic_query(rsx_path, source, theme, pos, |a, path, text, line, col| {
                    Some(a.completions_at(&path, text, line, col))
                })
                .await?
            }
            Section::View => {
                self.view_query(rsx_path, source, theme, pos, |a, path, text, offset| {
                    Some(a.completions_at_offset(&path, text, offset))
                })
                .await?
            }
            _ => return None,
        };
        Some(CompletionResponse::Array(self.defer_completion_docs(items)))
    }

    /// Strips each rust-analyzer completion item's documentation into the resolve cache (keyed by a fresh generation + index) and tags the item with that key in `data`. The list goes out lean; `completion_resolve` puts the docs back when the client asks for a specific item.
    fn defer_completion_docs(&self, items: Vec<CompletionItem>) -> Vec<CompletionItem> {
        let mut cache = self.completion_cache.lock().unwrap();
        cache.generation = cache.generation.wrapping_add(1);
        let generation = cache.generation;
        cache.docs.clear();
        items
            .into_iter()
            .enumerate()
            .map(|(i, mut item)| {
                cache.docs.push(item.documentation.take());
                item.data = Some(json!({ "g": generation, "i": i }));
                item
            })
            .collect()
    }

    /// `completionItem/resolve`: re-attach the documentation deferred by [`defer_completion_docs`]. A stale `data` (its batch superseded by a newer completion) simply resolves to no docs.
    pub fn completion_resolve(&self, mut item: CompletionItem) -> CompletionItem {
        let key = item
            .data
            .as_ref()
            .and_then(|data| Some((data.get("g")?.as_u64()?, data.get("i")?.as_u64()? as usize)));
        if let Some((generation, index)) = key
            && item.documentation.is_none()
        {
            let cache = self.completion_cache.lock().unwrap();
            if cache.generation == generation
                && let Some(Some(docs)) = cache.docs.get(index)
            {
                item.documentation = Some(docs.clone());
            }
        }
        // The client echoes `data` back on resolve; it has served its purpose, so drop it from the committed item.
        item.data = None;
        item
    }

    /// `textDocument/selectionRange`: smart-expand selection, from structure (parse-free).
    pub async fn selection_range(
        &self,
        params: SelectionRangeParams,
    ) -> Option<Vec<SelectionRange>> {
        let uri = &params.text_document.uri;
        let source = {
            let store = self.store.read().await;
            store.latest_source(uri).cloned()
        }?;
        Some(crate::analysis::selection_range::selection_ranges(
            &source,
            &params.positions,
        ))
    }

    /// `textDocument/rangeFormatting`: reformat only the hunks overlapping the requested range.
    pub async fn range_formatting(
        &self,
        params: DocumentRangeFormattingParams,
    ) -> Option<Vec<TextEdit>> {
        let uri = &params.text_document.uri;
        let source = {
            let store = self.store.read().await;
            store.latest_source(uri).cloned()
        }?;
        let to_format = source.clone();
        let formatted =
            tokio::task::spawn_blocking(move || crate::format::format_document(&to_format))
                .await
                .ok()
                .flatten()?;
        if formatted == source {
            return Some(vec![]);
        }
        Some(crate::format::range_edits(
            &source,
            &formatted,
            params.range,
        ))
    }

    pub async fn signature_help(&self, params: SignatureHelpParams) -> Option<SignatureHelp> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let file_path = crate::uri::to_path(uri);

        let (source, theme) = {
            let store = self.store.read().await;
            let parsed = store.get(uri)?;
            let project = file_path.as_deref().and_then(ProjectInfo::discover);
            let theme = project.as_ref().and_then(|p| p.theme_type.clone());
            (parsed.source.clone(), theme)
        };

        let rsx_path = file_path?;
        match find_section_at(&source, pos.line) {
            Section::Logic => {
                self.logic_query(rsx_path, source, theme, pos, |a, path, text, line, col| {
                    a.signature_help_at(&path, text, line, col)
                })
                .await
            }
            Section::View => {
                self.view_query(rsx_path, source, theme, pos, |a, path, text, offset| {
                    a.signature_help_at_offset(&path, text, offset)
                })
                .await
            }
            _ => None,
        }
    }

    /// Starts the (slow) workspace load on a blocking thread if it hasn't started yet. Returns immediately; queries that arrive while loading simply yield nothing.
    fn ensure_loading(&self, root: PathBuf, warm: Option<PathBuf>) {
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

    /// Maps a `[logic]` cursor into the generated module and runs `run` against the embedded analyzer on a blocking thread (the query is synchronous; the load may still be in flight, in which case this yields `None`).
    async fn logic_query<T, F>(
        &self,
        rsx_path: PathBuf,
        source: String,
        theme: Option<String>,
        pos: Position,
        run: F,
    ) -> Option<T>
    where
        F: FnOnce(&mut EmbeddedAnalyzer, PathBuf, String, u32, u32) -> Option<T> + Send + 'static,
        T: Send + 'static,
    {
        let crate::build_sync::GeneratedTarget {
            path: gen_path,
            code: gen_text,
            map,
            ..
        } = crate::build_sync::generated_target(&rsx_path, &source, theme.as_deref())?;
        // First generated line that originated from this `.rsx` line.
        let gen_line = map.iter().position(|m| *m == Some(pos.line))? as u32;
        let gen_col = pos.character + crate::ra::logic_indent();

        let root = crate::build_sync::crate_root(&rsx_path)?;
        self.ensure_loading(root, Some(gen_path.clone()));

        let analyzer = self.analyzer.clone();
        let outgoing = self.outgoing.clone();
        tokio::task::spawn_blocking(move || {
            let lock_at = std::time::Instant::now();
            let mut state = analyzer.lock().ok()?;
            let lock_ms = lock_at.elapsed().as_millis();
            let AnalyzerState::Ready(a) = &mut *state else {
                return None;
            };
            // A generated module the graph doesn't know yet (e.g. a `.rsx` added since load): drop to Idle so the next query reloads the workspace.
            if !a.knows_file(&gen_path) {
                *state = AnalyzerState::Idle;
                return None;
            }
            let ra_at = std::time::Instant::now();
            let result = run(a, gen_path, gen_text, gen_line, gen_col);
            // Only surface slow queries — a healthy query is sub-100ms, so anything past 1s flags a regression (e.g. a cold cache) without spamming the output on every keystroke.
            let ra_ms = ra_at.elapsed().as_millis();
            if lock_ms > 1000 || ra_ms > 1000 {
                outgoing.log_message(
                    MessageType::INFO,
                    format!("rsx-analyzer: slow [logic] query — lock {lock_ms}ms, ra {ra_ms}ms"),
                );
            }
            result
        })
        .await
        .ok()
        .flatten()
    }

    /// Maps a `[view]` cursor into the generated module via the transpiler's expression-span map and runs `run` at the resulting byte offset. Returns `None` (so native element/attr completion is preserved) when the cursor sits outside every verbatim `[view]` expression. The generated offset is a UTF-8 char boundary by construction: the fragment is byte-identical in source and output, and the `.rsx` cursor is resolved on a char boundary.
    async fn view_query<T, F>(
        &self,
        rsx_path: PathBuf,
        source: String,
        theme: Option<String>,
        pos: Position,
        run: F,
    ) -> Option<T>
    where
        F: FnOnce(&mut EmbeddedAnalyzer, PathBuf, String, TextSize) -> Option<T> + Send + 'static,
        T: Send + 'static,
    {
        let crate::build_sync::GeneratedTarget {
            path: gen_path,
            code: gen_text,
            expr_spans,
            ..
        } = crate::build_sync::generated_target(&rsx_path, &source, theme.as_deref())?;
        let rsx_byte = byte_offset(&source, pos.line, pos.character)?;
        // The containing expression span; the inclusive upper bound lets the cursor sit right after the last character (the common completion position, e.g. `count.|`).
        let span = expr_spans.iter().find(|s| {
            rsx_byte >= s.rsx_start as usize && rsx_byte <= (s.rsx_start + s.len) as usize
        })?;
        let gen_offset = span.gen_start as usize + (rsx_byte - span.rsx_start as usize);
        let offset = TextSize::from(gen_offset as u32);

        let root = crate::build_sync::crate_root(&rsx_path)?;
        self.ensure_loading(root, Some(gen_path.clone()));

        let analyzer = self.analyzer.clone();
        let outgoing = self.outgoing.clone();
        tokio::task::spawn_blocking(move || {
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
            let result = run(a, gen_path, gen_text, offset);
            let ra_ms = ra_at.elapsed().as_millis();
            if lock_ms > 1000 || ra_ms > 1000 {
                outgoing.log_message(
                    MessageType::INFO,
                    format!("rsx-analyzer: slow [view] query — lock {lock_ms}ms, ra {ra_ms}ms"),
                );
            }
            result
        })
        .await
        .ok()
        .flatten()
    }

    /// Runs `run` against the embedded analyzer on a blocking thread with no position mapping — for queries that target an offset computed directly in the generated file (component rename probes the generated `fn`/`Props` definitions). Yields `None` while the workspace load is still in flight or the generated module is unknown (a `.rsx` added since load → drop to Idle to reload).
    async fn run_analyzer<T, F>(&self, gen_path: PathBuf, root: PathBuf, run: F) -> Option<T>
    where
        F: FnOnce(&mut EmbeddedAnalyzer) -> Option<T> + Send + 'static,
        T: Send + 'static,
    {
        self.ensure_loading(root, Some(gen_path.clone()));
        let analyzer = self.analyzer.clone();
        tokio::task::spawn_blocking(move || {
            let mut state = analyzer.lock().ok()?;
            let AnalyzerState::Ready(a) = &mut *state else {
                return None;
            };
            if !a.knows_file(&gen_path) {
                *state = AnalyzerState::Idle;
                return None;
            }
            run(a)
        })
        .await
        .ok()
        .flatten()
    }

    /// Find-all-references for the Rust symbol under a `[logic]`/`[view]` cursor, via the embedded analyzer. Returns raw [`RefTarget`]s in generated-file coordinates; the caller reverse-maps them.
    async fn rust_references(
        &self,
        rsx_path: PathBuf,
        source: String,
        theme: Option<String>,
        pos: Position,
    ) -> Option<Vec<RefTarget>> {
        match find_section_at(&source, pos.line) {
            Section::Logic => {
                self.logic_query(rsx_path, source, theme, pos, |a, path, text, line, col| {
                    a.references_at(&path, text, line, col)
                })
                .await
            }
            Section::View => {
                self.view_query(rsx_path, source, theme, pos, |a, path, text, offset| {
                    a.references_at_offset(&path, text, offset)
                })
                .await
            }
            _ => None,
        }
    }

    /// Find-all-references for the Rust symbol under the cursor, reverse-mapped to `.rsx` `Location`s: refs in this file's generated module map back through the line / expr-span maps; refs in real source files pass through verbatim; refs in *other* generated modules are dropped (a file-scoped `[logic]` symbol has none, and a component's cross-component Rust calls are renamed via the dedicated component-rename path instead). Returns `(locations, unmapped)`: the reverse-mapped reference `Location`s plus the count of generated-file references that couldn't be placed (see [`reverse_map_rust_refs`]). Read-only callers ignore `unmapped`; rename refuses when it is non-zero.
    async fn rust_reference_locations(
        &self,
        uri: &Uri,
        rsx_path: PathBuf,
        source: String,
        theme: Option<String>,
        pos: Position,
    ) -> Option<(Vec<Location>, usize)> {
        let refs = self
            .rust_references(rsx_path.clone(), source.clone(), theme.clone(), pos)
            .await?;
        let target = crate::build_sync::generated_target(&rsx_path, &source, theme.as_deref())?;
        Some(reverse_map_rust_refs(
            refs,
            &target.path,
            &target.code,
            &target.map,
            &target.expr_spans,
            &source,
            uri,
        ))
    }

    /// Renames a component (`<feature_card>` → `<new_name>`): the defining `.rsx` file, every markup usage (native cross-file scan), and every hand-written Rust reference to the generated `fn` / `Props` (via the embedded analyzer). Returns a `document_changes` edit so the file rename rides along with the text edits. `None` if the new name is not a valid identifier or no defining file is found. Cross-component bare-Rust calls to a *subdirectory* component aren't renamed (the tag model is file-stem-based; the generated fn name is the flattened path) — a documented limit.
    async fn rename_component(
        &self,
        old_name: &str,
        new_name: &str,
        uri: &Uri,
        theme: Option<String>,
    ) -> Option<WorkspaceEdit> {
        // The new name becomes a file stem + fn identifier, so it must be a bare identifier.
        let valid = !new_name.is_empty()
            && new_name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
            && !new_name.starts_with(|c: char| c.is_ascii_digit());
        if !valid {
            return None;
        }
        let path = crate::uri::to_path(uri)?;
        let root = rsx_workspace::find_rsx_root(&path)
            .or_else(|| rsx_workspace::find_workspace_root(&path))?;

        // Markup usages + the defining file, from the workspace `.rsx` index.
        let old = old_name.to_string();
        let refs = self
            .with_index(root.clone(), move |idx| idx.component_references(&old))
            .await?;

        let mut edits: std::collections::HashMap<Uri, Vec<TextEdit>> =
            std::collections::HashMap::new();
        let mut def_uri: Option<Uri> = None;
        for loc in refs {
            // The (0,0) marker `component_references` emits for the defining file is the file itself, not a text occurrence — capture it for the rename op, never as an edit.
            if loc.range.start == loc.range.end {
                def_uri = Some(loc.uri);
            } else {
                edits.entry(loc.uri).or_default().push(TextEdit {
                    range: loc.range,
                    new_text: new_name.to_string(),
                });
            }
        }
        let def_uri = def_uri?;
        let def_path = crate::uri::to_path(&def_uri)?;

        // Hand-written Rust references to the generated `fn` / `Props` (in real `.rs` files).
        for (ru, redits) in self
            .component_rust_edits(
                def_path.clone(),
                old_name.to_string(),
                new_name.to_string(),
                theme,
            )
            .await
        {
            edits.entry(ru).or_default().extend(redits);
        }

        // Text edits first, then the file rename, so edits against the old URI apply before it moves.
        let mut ops: Vec<DocumentChangeOperation> = edits
            .into_iter()
            .map(|(uri, edits)| {
                DocumentChangeOperation::Edit(TextDocumentEdit {
                    text_document: OptionalVersionedTextDocumentIdentifier { uri, version: None },
                    edits: edits.into_iter().map(OneOf::Left).collect(),
                })
            })
            .collect();
        let new_def_path = def_path.with_file_name(format!("{new_name}.rsx"));
        let new_def_uri = crate::uri::from_path(&new_def_path)?;
        ops.push(DocumentChangeOperation::Op(ResourceOp::Rename(
            RenameFile {
                old_uri: def_uri,
                new_uri: new_def_uri,
                options: None,
                annotation_id: None,
            },
        )));

        Some(WorkspaceEdit {
            changes: None,
            document_changes: Some(DocumentChanges::Operations(ops)),
            change_annotations: None,
        })
    }

    /// Text edits in real `.rs` files for hand-written references to a component's generated `fn`/`Props` (e.g. `crate::feature_card(...)` / `crate::FeatureCardProps { .. }`). Queries the embedded analyzer from the generated definitions; generated build files are skipped (rebuilt from the `.rsx`). Returns empty if the analyzer isn't ready (logged) or the definitions aren't found.
    async fn component_rust_edits(
        &self,
        def_path: PathBuf,
        old_name: String,
        new_name: String,
        theme: Option<String>,
    ) -> std::collections::HashMap<Uri, Vec<TextEdit>> {
        let mut edits: std::collections::HashMap<Uri, Vec<TextEdit>> =
            std::collections::HashMap::new();
        let Some(root) = crate::build_sync::crate_root(&def_path) else {
            return edits;
        };
        let Ok(source) = std::fs::read_to_string(&def_path) else {
            return edits;
        };
        let def_theme = ProjectInfo::discover(&def_path)
            .and_then(|p| p.theme_type.clone())
            .or(theme);
        let Some(target) =
            crate::build_sync::generated_target(&def_path, &source, def_theme.as_deref())
        else {
            return edits;
        };

        let fn_name = to_snake_case(&old_name);
        let new_fn = to_snake_case(&new_name);
        let props_type = to_pascal_case(&old_name) + "Props";
        let new_props = to_pascal_case(&new_name) + "Props";

        // Offsets of the generated definition names (`fn NAME(` skips "fn "; `struct NAME` skips "struct ").
        let Some(fn_offset) = target.code.find(&format!("fn {fn_name}(")).map(|i| i + 3) else {
            return edits;
        };
        let props_offset = target
            .code
            .find(&format!("struct {props_type}"))
            .map(|i| i + 7);

        let gen_path = target.path.clone();
        let gen_code = target.code.clone();
        let result = self
            .run_analyzer(gen_path.clone(), root, move |a| {
                let fn_refs = a
                    .references_at_offset(
                        &gen_path,
                        gen_code.clone(),
                        TextSize::from(fn_offset as u32),
                    )
                    .unwrap_or_default();
                let props_refs = props_offset
                    .map(|o| {
                        a.references_at_offset(
                            &gen_path,
                            gen_code.clone(),
                            TextSize::from(o as u32),
                        )
                        .unwrap_or_default()
                    })
                    .unwrap_or_default();
                Some((fn_refs, props_refs))
            })
            .await;

        let Some((fn_refs, props_refs)) = result else {
            self.outgoing.log_message(
                MessageType::INFO,
                "rsx-analyzer: analyzer not ready — component's Rust references were left unchanged"
                    .to_string(),
            );
            return edits;
        };

        for (refs, replacement) in [(fn_refs, new_fn.as_str()), (props_refs, new_props.as_str())] {
            for r in refs {
                // The generated module is rebuilt from the `.rsx`; only real source files need editing.
                if crate::build_sync::is_generated_build_file(&r.path) {
                    continue;
                }
                let Some(uri) = crate::uri::from_path(&r.path) else {
                    continue;
                };
                edits.entry(uri).or_default().push(TextEdit {
                    range: r.range,
                    new_text: replacement.to_string(),
                });
            }
        }
        edits
    }

    pub async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Option<GotoDefinitionResponse> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let file_path = crate::uri::to_path(uri);

        let (source, native, theme) = {
            let store = self.store.read().await;
            let parsed = store.get(uri)?;
            let project = file_path.as_deref().and_then(ProjectInfo::discover);
            // Native `.rsx` definitions (classes / colors / component files) win when present.
            let native = goto_definition(
                &parsed.document,
                &parsed.source,
                uri,
                pos.line,
                pos.character,
                project.as_ref(),
            );
            let theme = project.as_ref().and_then(|p| p.theme_type.clone());
            (parsed.source.clone(), native, theme)
        };

        if let Some(response) = native {
            return Some(response);
        }
        // Outside a native `.rsx` zone: resolve Rust definitions via the embedded rust-analyzer, then reverse-map any generated-`.rs` targets back onto their `.rsx` (see `map_definition_targets`).
        let rsx_path = file_path?;
        let targets = match find_section_at(&source, pos.line) {
            Section::Logic => {
                self.logic_query(
                    rsx_path.clone(),
                    source,
                    theme,
                    pos,
                    |a, path, text, line, col| a.definition_at(&path, text, line, col),
                )
                .await?
            }
            Section::View => {
                self.view_query(
                    rsx_path.clone(),
                    source,
                    theme,
                    pos,
                    |a, path, text, offset| a.definition_at_offset(&path, text, offset),
                )
                .await?
            }
            _ => return None,
        };
        let locations = map_definition_targets(targets);
        if locations.is_empty() {
            return None;
        }
        Some(GotoDefinitionResponse::Array(locations))
    }

    pub async fn hover(&self, params: HoverParams) -> Option<Hover> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let file_path = crate::uri::to_path(uri);

        let (source, native, theme) = {
            let store = self.store.read().await;
            let parsed = store.get(uri)?;
            let project = file_path.as_deref().and_then(ProjectInfo::discover);
            let native = hover_info(
                &parsed.document,
                &parsed.source,
                pos.line,
                pos.character,
                project.as_ref(),
            );
            let theme = project.as_ref().and_then(|p| p.theme_type.clone());
            (parsed.source.clone(), native, theme)
        };

        if let Some(hover) = native {
            return Some(hover);
        }
        // Native `.rsx` hover (tags / colors) didn't match: delegate to the embedded rust-analyzer over the generated module — line-mapped for `[logic]`, expression-span-mapped for `[view]`.
        let rsx_path = file_path?;
        match find_section_at(&source, pos.line) {
            Section::Logic => {
                self.logic_query(rsx_path, source, theme, pos, |a, path, text, line, col| {
                    a.hover_at(&path, text, line, col)
                })
                .await
            }
            Section::View => {
                self.view_query(rsx_path, source, theme, pos, |a, path, text, offset| {
                    a.hover_at_offset(&path, text, offset)
                })
                .await
            }
            _ => None,
        }
    }

    pub async fn formatting(&self, params: DocumentFormattingParams) -> Option<Vec<TextEdit>> {
        let uri = &params.text_document.uri;

        // Format the live buffer, not the last good parse: if the current text does not parse, `format_document` returns `None` below and we emit no edit, leaving the file untouched.
        let source = {
            let store = self.store.read().await;
            store.latest_source(uri).cloned()
        }?;

        // rustfmt is spawned synchronously, so format off the async runtime thread.
        let to_format = source.clone();
        let formatted =
            tokio::task::spawn_blocking(move || crate::format::format_document(&to_format))
                .await
                .ok()
                .flatten()?;
        if formatted == source {
            return Some(vec![]);
        }

        Some(vec![TextEdit {
            range: full_document_range(&source),
            new_text: formatted,
        }])
    }

    /// `textDocument/documentColor`: inline swatches for color literals in `[style]`/`[view]`.
    pub async fn document_color(&self, params: DocumentColorParams) -> Vec<ColorInformation> {
        let uri = &params.text_document.uri;
        let store = self.store.read().await;
        let Some(parsed) = store.get(uri) else {
            return Vec::new();
        };
        crate::analysis::color::document_colors(&parsed.document, &parsed.source)
    }

    /// `textDocument/colorPresentation`: the picker write-back — the chosen color as a hex string.
    pub fn color_presentation(&self, params: ColorPresentationParams) -> Vec<ColorPresentation> {
        crate::analysis::color::color_presentations(params.color)
    }

    /// `textDocument/documentSymbol`: the outline of `[style]` constants/classes and `[preview]`s.
    pub async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Option<DocumentSymbolResponse> {
        let uri = &params.text_document.uri;
        let store = self.store.read().await;
        let parsed = store.get(uri)?;
        let symbols = crate::analysis::symbols::document_symbols(&parsed.document, &parsed.source);
        Some(DocumentSymbolResponse::Nested(symbols))
    }

    /// `textDocument/foldingRange`: section + indentation folds, over the live buffer (parse-free).
    pub async fn folding_range(&self, params: FoldingRangeParams) -> Option<Vec<FoldingRange>> {
        let uri = &params.text_document.uri;
        let source = {
            let store = self.store.read().await;
            store.latest_source(uri).cloned()
        }?;
        Some(crate::analysis::folding::folding_ranges(&source))
    }

    /// `textDocument/codeAction`: quick fixes synthesized from the request's context diagnostics.
    pub async fn code_action(&self, params: CodeActionParams) -> Option<Vec<CodeActionOrCommand>> {
        let uri = &params.text_document.uri;
        let source = {
            let store = self.store.read().await;
            store.latest_source(uri).cloned()
        }?;
        let actions =
            crate::analysis::code_action::code_actions(&source, uri, &params.context.diagnostics);
        (!actions.is_empty()).then_some(actions)
    }

    /// `textDocument/codeLens`: a "▶ Preview" lens over each `[preview …]` section.
    pub async fn code_lens(&self, params: CodeLensParams) -> Option<Vec<CodeLens>> {
        let uri = &params.text_document.uri;
        let store = self.store.read().await;
        let parsed = store.get(uri)?;
        Some(crate::analysis::lens::code_lenses(&parsed.document, uri))
    }

    /// `textDocument/documentLink`: clickable links for `img src:"…"` asset paths that exist on disk.
    pub async fn document_link(&self, params: DocumentLinkParams) -> Option<Vec<DocumentLink>> {
        let uri = &params.text_document.uri;
        let file_dir = crate::uri::to_path(uri)?.parent()?.to_path_buf();
        let store = self.store.read().await;
        let parsed = store.get(uri)?;
        Some(crate::analysis::links::document_links(
            &parsed.document,
            &parsed.source,
            &file_dir,
        ))
    }

    /// `textDocument/inlayHint`: type/parameter hints from the embedded analyzer, mapped back onto the `[logic]` zone. `[view]`-origin hints are dropped (the generated builder has no line-stable column correspondence), so hints appear only where the mapping is exact.
    pub async fn inlay_hint(&self, params: InlayHintParams) -> Option<Vec<InlayHint>> {
        let uri = &params.text_document.uri;
        let file_path = crate::uri::to_path(uri)?;
        let (source, theme) = {
            let store = self.store.read().await;
            let source = store.get(uri)?.source.clone();
            let theme = ProjectInfo::discover(&file_path).and_then(|p| p.theme_type.clone());
            (source, theme)
        };
        let target = crate::build_sync::generated_target(&file_path, &source, theme.as_deref())?;
        let root = crate::build_sync::crate_root(&file_path)?;
        let gen_path = target.path.clone();
        let gen_code = target.code.clone();
        let raws = self
            .run_analyzer(gen_path.clone(), root, move |a| {
                Some(a.inlay_hints(&gen_path, gen_code))
            })
            .await?;

        let mut out = Vec::new();
        for raw in raws {
            let Some(Some(rsx_line)) = target.map.get(raw.line as usize) else {
                continue;
            };
            let rsx_line = *rsx_line;
            if find_section_at(&source, rsx_line) != Section::Logic {
                continue;
            }
            let gen_line_text = nth_line(&target.code, raw.line as usize).unwrap_or("");
            let rsx_line_text = nth_line(&source, rsx_line as usize).unwrap_or("");
            let delta =
                leading_ws_utf16(gen_line_text).saturating_sub(leading_ws_utf16(rsx_line_text));
            out.push(InlayHint {
                position: Position {
                    line: rsx_line,
                    character: raw.col.saturating_sub(delta),
                },
                label: InlayHintLabel::String(raw.label),
                kind: raw.kind,
                text_edits: None,
                tooltip: None,
                padding_left: Some(raw.pad_left),
                padding_right: Some(raw.pad_right),
                data: None,
            });
        }
        Some(out)
    }

    /// `textDocument/documentHighlight`: every occurrence of the symbol under the cursor — `@class` and `$signal` natively, or a Rust identifier in `[logic]`/`[view]` via the embedded analyzer (refs landing in this file).
    pub async fn document_highlight(
        &self,
        params: DocumentHighlightParams,
    ) -> Option<Vec<DocumentHighlight>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let file_path = crate::uri::to_path(uri);
        let (source, theme) = {
            let store = self.store.read().await;
            let source = store.latest_source(uri).cloned()?;
            let theme = file_path
                .as_deref()
                .and_then(ProjectInfo::discover)
                .and_then(|p| p.theme_type.clone());
            (source, theme)
        };
        let occurrences = if let Some(name) =
            crate::analysis::occurrences::class_at(&source, pos.line, pos.character)
        {
            crate::analysis::occurrences::class_occurrences(&source, &name)
        } else if let Some(name) =
            crate::analysis::occurrences::signal_at(&source, pos.line, pos.character)
        {
            crate::analysis::occurrences::signal_occurrences(&source, &name)
        } else {
            // Rust symbol: keep only references that map back into this document.
            let rsx_path = file_path?;
            let (locations, _) = self
                .rust_reference_locations(uri, rsx_path, source, theme, pos)
                .await?;
            locations
                .into_iter()
                .filter(|l| &l.uri == uri)
                .map(|l| l.range)
                .collect()
        };
        Some(
            occurrences
                .into_iter()
                .map(|range| DocumentHighlight {
                    range,
                    kind: Some(DocumentHighlightKind::TEXT),
                })
                .collect(),
        )
    }

    /// `textDocument/references`: every use of the symbol under the cursor — `@class`/`$signal` (file-scoped) and component tags (cross-file) natively, or a Rust identifier in `[logic]`/`[view]` via the embedded analyzer.
    pub async fn references(&self, params: ReferenceParams) -> Option<Vec<Location>> {
        let uri = &params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let file_path = crate::uri::to_path(uri);
        let (source, theme) = {
            let store = self.store.read().await;
            let source = store.latest_source(uri).cloned()?;
            let theme = file_path
                .as_deref()
                .and_then(ProjectInfo::discover)
                .and_then(|p| p.theme_type.clone());
            (source, theme)
        };
        // File-scoped style class → single-file occurrences.
        if let Some(name) = crate::analysis::occurrences::class_at(&source, pos.line, pos.character)
        {
            let locations = crate::analysis::occurrences::class_occurrences(&source, &name)
                .into_iter()
                .map(|range| Location {
                    uri: uri.clone(),
                    range,
                })
                .collect();
            return Some(locations);
        }
        // File-scoped signal → single-file occurrences ([logic] decl/uses + `$name` in [view]).
        if let Some(name) =
            crate::analysis::occurrences::signal_at(&source, pos.line, pos.character)
        {
            let locations = crate::analysis::occurrences::signal_occurrences(&source, &name)
                .into_iter()
                .map(|range| Location {
                    uri: uri.clone(),
                    range,
                })
                .collect();
            return Some(locations);
        }
        // Component tag → cross-file references (its `.rsx` plus every `<tag>` usage).
        if let Some(name) =
            crate::analysis::occurrences::component_at(&source, pos.line, pos.character)
        {
            let path = crate::uri::to_path(uri)?;
            let root = rsx_workspace::find_rsx_root(&path)
                .or_else(|| rsx_workspace::find_workspace_root(&path))?;
            let locations = self
                .with_index(root, move |idx| idx.component_references(&name))
                .await?;
            return (!locations.is_empty()).then_some(locations);
        }
        // Rust symbol in `[logic]`/`[view]` → analyzer find-all-references, reverse-mapped.
        let rsx_path = file_path?;
        let (locations, _) = self
            .rust_reference_locations(uri, rsx_path, source, theme, pos)
            .await?;
        (!locations.is_empty()).then_some(locations)
    }

    /// `textDocument/prepareRename`: confirm the cursor is on a renameable symbol — `@class`, `$signal`, a component tag, or a Rust identifier in `[logic]`/`[view]` — and return the range to edit.
    pub async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Option<PrepareRenameResponse> {
        let uri = &params.text_document.uri;
        let pos = params.position;
        let source = {
            let store = self.store.read().await;
            store.latest_source(uri).cloned()
        }?;
        let range = if crate::analysis::occurrences::class_at(&source, pos.line, pos.character)
            .is_some()
        {
            crate::analysis::occurrences::occurrence_at(&source, pos.line, pos.character)?
        } else if crate::analysis::occurrences::signal_at(&source, pos.line, pos.character)
            .is_some()
        {
            crate::analysis::occurrences::signal_occurrence_at(&source, pos.line, pos.character)?
        } else if crate::analysis::occurrences::component_at(&source, pos.line, pos.character)
            .is_some()
        {
            crate::analysis::occurrences::component_at_range(&source, pos.line, pos.character)?
        } else if matches!(
            find_section_at(&source, pos.line),
            Section::Logic | Section::View
        ) {
            // Rust identifier under the cursor; rename verifies the analyzer resolves it (else no edit).
            let line_text = source.lines().nth(pos.line as usize)?;
            let (start, word) = ident_at(line_text, pos.character)?;
            name_range(pos.line, line_text, start, word.len())
        } else {
            return None;
        };
        Some(PrepareRenameResponse::Range(range))
    }

    /// `textDocument/rename`: rewrite every occurrence of the symbol under the cursor. `@class`/`$signal` are single-file text rewrites; a component tag renames its `.rsx` file + markup usages + Rust references (cross-file); a Rust identifier in `[logic]`/`[view]` is renamed via the analyzer's find-all-references, reverse-mapped onto the `.rsx` (and any real `.rs` files).
    pub async fn rename(&self, params: RenameParams) -> Option<WorkspaceEdit> {
        let uri = &params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let new_name = params.new_name;
        let file_path = crate::uri::to_path(uri);
        let (source, theme) = {
            let store = self.store.read().await;
            let source = store.latest_source(uri).cloned()?;
            let theme = file_path
                .as_deref()
                .and_then(ProjectInfo::discover)
                .and_then(|p| p.theme_type.clone());
            (source, theme)
        };

        // File-scoped class / signal → single-file text rewrite.
        let single_file = if let Some(name) =
            crate::analysis::occurrences::class_at(&source, pos.line, pos.character)
        {
            Some(crate::analysis::occurrences::class_occurrences(
                &source, &name,
            ))
        } else {
            crate::analysis::occurrences::signal_at(&source, pos.line, pos.character)
                .map(|name| crate::analysis::occurrences::signal_occurrences(&source, &name))
        };
        if let Some(occurrences) = single_file {
            let edits: Vec<TextEdit> = occurrences
                .into_iter()
                .map(|range| TextEdit {
                    range,
                    new_text: new_name.clone(),
                })
                .collect();
            if edits.is_empty() {
                return None;
            }
            let mut changes = std::collections::HashMap::new();
            changes.insert(uri.clone(), edits);
            return Some(WorkspaceEdit {
                changes: Some(changes),
                document_changes: None,
                change_annotations: None,
            });
        }

        // Component tag → cross-file rename (file + markup + Rust references).
        if let Some(name) =
            crate::analysis::occurrences::component_at(&source, pos.line, pos.character)
        {
            return self.rename_component(&name, &new_name, uri, theme).await;
        }

        // Rust identifier in `[logic]`/`[view]` → analyzer rename, reverse-mapped per file.
        if matches!(
            find_section_at(&source, pos.line),
            Section::Logic | Section::View
        ) {
            let rsx_path = file_path?;
            let (locations, unmapped) = self
                .rust_reference_locations(uri, rsx_path, source, theme, pos)
                .await?;
            if locations.is_empty() {
                return None;
            }
            // A partial rename would leave the code uncompilable. If any reference couldn't be precisely located (a non-verbatim `[view]` use, or another component's generated module), refuse the whole rename rather than half-apply it.
            if unmapped > 0 {
                self.outgoing.log_message(
                    MessageType::INFO,
                    format!(
                        "rsx-analyzer: rename skipped — {unmapped} reference(s) couldn't be precisely located (non-verbatim [view] usage or cross-component). Rename left unchanged."
                    ),
                );
                return None;
            }
            let mut changes: std::collections::HashMap<Uri, Vec<TextEdit>> =
                std::collections::HashMap::new();
            for loc in locations {
                changes.entry(loc.uri).or_default().push(TextEdit {
                    range: loc.range,
                    new_text: new_name.clone(),
                });
            }
            return Some(WorkspaceEdit {
                changes: Some(changes),
                document_changes: None,
                change_annotations: None,
            });
        }

        None
    }

    /// `workspace/symbol`: components and `@classes` across the project, filtered by the query.
    pub async fn workspace_symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Option<Vec<SymbolInformation>> {
        let query = params.query;
        let root = {
            let store = self.store.read().await;
            let uri = store.any_uri()?;
            let path = crate::uri::to_path(uri)?;
            // Prefer an `rsx.toml` root, but fall back to the Cargo workspace root so apps without an `rsx.toml` (e.g. a themed app configured in code) still get workspace symbols.
            rsx_workspace::find_rsx_root(&path)
                .or_else(|| rsx_workspace::find_workspace_root(&path))?
        };
        self.with_index(root, move |idx| idx.symbols(&query)).await
    }

    /// `textDocument/semanticTokens/full`: parse-aware highlighting over the live buffer.
    pub async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Option<SemanticTokensResult> {
        let uri = &params.text_document.uri;
        let source = {
            let store = self.store.read().await;
            store.latest_source(uri).cloned()
        }?;
        let data = crate::analysis::semantic_tokens::semantic_tokens(&source);
        Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data,
        }))
    }
}

/// Maps rust-analyzer definition targets to `.rsx` `Location`s, handling three cases per target: (1) the generated `.rs` for *this* `.rsx` and (2) *another* component's `.rsx/build/*.rs` are both reverse-mapped through that build file's line map (`generated line → .rsx line`) onto its `.rsx` source — a generated line with no originating `.rsx` line is dropped; (3) any other path (a dependency, std, or a hand-written `.rs`) is returned verbatim in its own coordinates.
fn map_definition_targets(targets: Vec<DefinitionTarget>) -> Vec<Location> {
    let mut locations = Vec::new();
    for target in targets {
        if crate::build_sync::is_generated_build_file(&target.path) {
            // Cases 1 & 2: a generated build file → walk back to its `.rsx` via the sibling `.rs.map`.
            let Some((rsx_path, map)) = crate::build_sync::rsx_source_and_map(&target.path) else {
                continue;
            };
            let Some(Some(rsx_line)) = map.get(target.range.start.line as usize) else {
                continue;
            };
            if let Some(uri) = crate::uri::from_path(&rsx_path) {
                locations.push(Location {
                    uri,
                    range: Range {
                        start: Position {
                            line: *rsx_line,
                            character: 0,
                        },
                        end: Position {
                            line: *rsx_line,
                            character: 0,
                        },
                    },
                });
            }
        } else if let Some(uri) = crate::uri::from_path(&target.path) {
            // Case 3: a real source file → jump straight to its own range.
            locations.push(Location {
                uri,
                range: target.range,
            });
        }
    }
    locations
}

/// Reverse-maps analyzer references onto `.rsx` `Location`s with precise ranges: a real source file passes through verbatim; a reference in *this* file's generated module maps back through the expr-span map (`[view]` verbatim expressions) or the line map (`[logic]` / Props struct); a reference in *another* generated module can't be precisely mapped here. Duplicates are coalesced. Returns `(locations, unmapped)` where `unmapped` counts generated-file references that produced no location — non-zero means the result is incomplete, so a rename must refuse rather than half-apply.
fn reverse_map_rust_refs(
    targets: Vec<RefTarget>,
    current_gen_path: &std::path::Path,
    gen_code: &str,
    map: &[Option<u32>],
    expr_spans: &[ExprSpan],
    rsx_source: &str,
    rsx_uri: &Uri,
) -> (Vec<Location>, usize) {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    let mut unmapped = 0usize;
    for target in targets {
        let is_generated = crate::build_sync::is_generated_build_file(&target.path);
        let location = if !is_generated {
            crate::uri::from_path(&target.path).map(|uri| Location {
                uri,
                range: target.range,
            })
        } else if target.path == current_gen_path {
            reverse_map_current_file(&target, gen_code, map, expr_spans, rsx_source).map(|range| {
                Location {
                    uri: rsx_uri.clone(),
                    range,
                }
            })
        } else {
            None
        };
        match location {
            Some(location) => {
                let key = (
                    location.uri.as_str().to_string(),
                    location.range.start.line,
                    location.range.start.character,
                    location.range.end.character,
                );
                if seen.insert(key) {
                    out.push(location);
                }
            }
            // A generated-file reference we couldn't place (non-verbatim `[view]` fragment, or another component's module) → the reverse-map is lossy here; flag it for the rename guard.
            None if is_generated => unmapped += 1,
            None => {}
        }
    }
    (out, unmapped)
}

/// Reverse-maps one generated-file reference span back onto the current `.rsx`. `[view]` verbatim expressions map byte-for-byte through their `ExprSpan`; everything else is line-mapped, with the column shifted by the leading-whitespace delta between the generated and `.rsx` lines (`+4` for `[logic]`, `0` for the verbatim Props struct). Returns `None` when the generated line has no `.rsx` origin (boilerplate / transpiler-injected).
fn reverse_map_current_file(
    target: &RefTarget,
    gen_code: &str,
    map: &[Option<u32>],
    expr_spans: &[ExprSpan],
    rsx_source: &str,
) -> Option<Range> {
    if let Some(span) = expr_spans
        .iter()
        .find(|s| target.byte_start >= s.gen_start && target.byte_start < s.gen_start + s.len)
    {
        let span_end = span.gen_start + span.len;
        let rsx_start = span.rsx_start + (target.byte_start - span.gen_start);
        let rsx_end = span.rsx_start + (target.byte_end.min(span_end) - span.gen_start);
        return Some(Range {
            start: offset_to_position(rsx_source, rsx_start as usize),
            end: offset_to_position(rsx_source, rsx_end as usize),
        });
    }
    let gen_line = target.range.start.line as usize;
    let rsx_line = (*map.get(gen_line)?)?;
    // The line-map + indent-delta column math only holds for `[logic]` (lines emitted verbatim under a fixed indent, incl. the Props struct). A `[view]`/`[preview]` reference that fell through the expr-span check above (e.g. an `img src:foo` attr value) has no column correspondence — drop it rather than emit a bogus range that would mis-highlight and corrupt a rename.
    if find_section_at(rsx_source, rsx_line) != Section::Logic {
        return None;
    }
    let gen_line_text = nth_line(gen_code, gen_line)?;
    let rsx_line_text = nth_line(rsx_source, rsx_line as usize).unwrap_or("");
    let delta = leading_ws_utf16(gen_line_text).saturating_sub(leading_ws_utf16(rsx_line_text));
    Some(Range {
        start: Position {
            line: rsx_line,
            character: target.range.start.character.saturating_sub(delta),
        },
        end: Position {
            line: rsx_line,
            character: target.range.end.character.saturating_sub(delta),
        },
    })
}

/// The `n`-th line of `text` (0-based), without its trailing newline.
fn nth_line(text: &str, n: usize) -> Option<&str> {
    text.split_inclusive('\n')
        .nth(n)
        .map(|line| line.strip_suffix('\n').unwrap_or(line))
}

/// Width (UTF-16 code units) of the leading space/tab run of `line`.
fn leading_ws_utf16(line: &str) -> u32 {
    line.chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .map(|c| c.len_utf16() as u32)
        .sum()
}

/// Builds the range covering all of `source`, used to replace the whole document with its formatted form. Character offsets are UTF-16 code units, per LSP.
fn full_document_range(source: &str) -> Range {
    let mut line = 0u32;
    let mut last_line_len = 0u32;
    for chunk in source.split_inclusive('\n') {
        if chunk.ends_with('\n') {
            line += 1;
            last_line_len = 0;
        } else {
            last_line_len = chunk.encode_utf16().count() as u32;
        }
    }
    Range {
        start: Position {
            line: 0,
            character: 0,
        },
        end: Position {
            line,
            character: last_line_len,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(
        gen_line: u32,
        gen_char_start: u32,
        gen_char_end: u32,
        bytes: (u32, u32),
    ) -> RefTarget {
        RefTarget {
            path: PathBuf::from("/x/.rsx/build/c.rs"),
            byte_start: bytes.0,
            byte_end: bytes.1,
            range: Range {
                start: Position {
                    line: gen_line,
                    character: gen_char_start,
                },
                end: Position {
                    line: gen_line,
                    character: gen_char_end,
                },
            },
        }
    }

    #[test]
    fn logic_ref_maps_back_subtracting_the_indent() {
        // Generated `[logic]` lines carry a +4 indent; the line map ties gen line 1 to .rsx line 1.
        let gen_src = "boilerplate\n    let total = x;\n";
        let rsx = "[logic]\nlet total = x;\n";
        let map = vec![None, Some(1)];
        // `total` sits at gen col 8 (`    let `); expect rsx col 4 (`let `).
        let t = target(1, 8, 13, (0, 0));
        let range = reverse_map_current_file(&t, gen_src, &map, &[], rsx).unwrap();
        assert_eq!(range.start.line, 1);
        assert_eq!(range.start.character, 4);
        assert_eq!(range.end.character, 9);
    }

    #[test]
    fn view_ref_maps_through_the_expr_span() {
        // A verbatim `[view]` expression: the `name` fragment is byte-identical in source and gen.
        let rsx = "[view]\ncol\n    text \"{name}\"\n";
        let rsx_name_byte = rsx.find("name").unwrap() as u32;
        let gen_src = "fn c() {\n    text(format!(\"{}\", name))\n}\n";
        let gen_name_byte = gen_src.find("name").unwrap() as u32;
        let spans = vec![ExprSpan {
            rsx_start: rsx_name_byte,
            len: 4,
            gen_start: gen_name_byte,
        }];
        let t = target(1, 0, 0, (gen_name_byte, gen_name_byte + 4));
        let range = reverse_map_current_file(&t, gen_src, &[], &spans, rsx).unwrap();
        // `name` is on .rsx line 2 (`    text "{name}"`), at the `{`+1 column.
        assert_eq!(range.start.line, 2);
        let col = "    text \"{".encode_utf16().count() as u32;
        assert_eq!(range.start.character, col);
        assert_eq!(range.end.character, col + 4);
    }

    #[test]
    fn boilerplate_lines_have_no_origin() {
        let gen_src = "boilerplate\n";
        let map = vec![None];
        let t = target(0, 0, 3, (0, 0));
        assert!(reverse_map_current_file(&t, gen_src, &map, &[], "x\n").is_none());
    }

    #[test]
    fn real_files_pass_through_generated_files_reverse_map() {
        let uri: Uri = "file:///x/src/c.rsx".parse().unwrap();
        let real = RefTarget {
            path: PathBuf::from("/x/src/lib.rs"),
            byte_start: 0,
            byte_end: 4,
            range: Range {
                start: Position {
                    line: 5,
                    character: 2,
                },
                end: Position {
                    line: 5,
                    character: 6,
                },
            },
        };
        let (locs, unmapped) = reverse_map_rust_refs(
            vec![real],
            std::path::Path::new("/x/.rsx/build/c.rs"),
            "",
            &[],
            &[],
            "",
            &uri,
        );
        assert_eq!(locs.len(), 1);
        assert!(locs[0].uri.as_str().ends_with("lib.rs"));
        assert_eq!(locs[0].range.start.line, 5);
        assert_eq!(unmapped, 0);
    }

    #[test]
    fn view_ref_without_a_span_is_dropped_not_corrupted() {
        // A `[view]` reference outside any verbatim expr-span (e.g. an `img src:foo` attr value) must be dropped and counted as unmapped — never mapped with a bogus column (which corrupted renames).
        let uri: Uri = "file:///x/src/c.rsx".parse().unwrap();
        let gen_path = std::path::Path::new("/x/.rsx/build/c.rs");
        let rsx = "[view]\ncol\n    img src:foo\n";
        let gen_src = "fn c() {\n    let __src = foo.clone();\n}\n";
        // gen line 1 → rsx line 2 (`    img src:foo`, a `[view]` line).
        let map = vec![None, Some(2), None];
        let t = RefTarget {
            path: gen_path.to_path_buf(),
            byte_start: 0,
            byte_end: 0,
            range: Range {
                start: Position {
                    line: 1,
                    character: 15,
                },
                end: Position {
                    line: 1,
                    character: 18,
                },
            },
        };
        let (locs, unmapped) =
            reverse_map_rust_refs(vec![t], gen_path, gen_src, &map, &[], rsx, &uri);
        assert!(locs.is_empty());
        assert_eq!(unmapped, 1);
    }
}
