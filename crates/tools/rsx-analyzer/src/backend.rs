use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use lsp_types::*;
use ra_ap_ide::TextSize;
use rsx_diagnostics::semantic_diagnostics;
use tokio::sync::RwLock;

use crate::analysis::completions::{
    CompletionKind, attribute_key_items, color_items, completion_context, element_name_items,
    style_class_items,
};
use crate::analysis::definition::goto_definition;
use crate::analysis::hover::hover_info;
use crate::position::{Section, find_section_at};
use crate::project::ProjectInfo;
use crate::ra::{DefinitionTarget, EmbeddedAnalyzer};
use crate::rpc::OutgoingSender;
use crate::store::Store;

/// Lifecycle of the embedded rust-analyzer: loaded lazily on the first `[logic]`
/// query because `load()` is slow (cargo metadata + crate graph).
// Always lives behind `Arc<Mutex<…>>` and is only ever written in place, so the large `Ready` variant
// is never moved by value — the size disparity clippy flags is irrelevant here.
#[allow(clippy::large_enum_variant)]
enum AnalyzerState {
    Idle,
    Loading,
    Ready(EmbeddedAnalyzer),
    Failed,
}

pub struct Backend {
    outgoing: OutgoingSender,
    store: Arc<RwLock<Store>>,
    analyzer: Arc<Mutex<AnalyzerState>>,
    // Monotonic edit counter, bumped on every reparse. A spawned diagnostics task captures the value
    // it was queued for and bails before the expensive rust-analyzer query if a newer edit superseded
    // it, so keystroke-rate edits don't pile up redundant `full_diagnostics` runs behind the lock.
    revision: Arc<AtomicU64>,
}

impl Backend {
    pub fn new(outgoing: OutgoingSender) -> Self {
        Self {
            outgoing,
            store: Arc::new(RwLock::new(Store::new())),
            analyzer: Arc::new(Mutex::new(AnalyzerState::Idle)),
            revision: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn outgoing(&self) -> &OutgoingSender {
        &self.outgoing
    }

    async fn reparse_and_diagnose(&self, uri: Uri, text: String) -> Vec<Diagnostic> {
        let revision = self.revision.fetch_add(1, Ordering::Relaxed) + 1;
        let file_path = crate::uri::to_path(&uri);
        // Hold the store lock only for the parse + native diagnostics + build-file sync, then release it
        // before the (slower) rust-analyzer query so concurrent completion/hover reads aren't blocked.
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
            // Mirror the live buffer to its generated `.rs` so the workspace rust-analyzer analyzes
            // the in-flight text — this is what makes completion/hover/definition live instead of one
            // `cargo check` behind. Same output as the `app!` macro produces at compile time.
            let theme = project.as_ref().and_then(|p| p.theme_type.clone());
            if let Some(rsx_path) = file_path.as_deref() {
                crate::build_sync::sync_build_file(rsx_path, &parsed.source, theme.as_deref());
            }
            (semantic, parsed.source.clone(), theme)
        };

        let native: Vec<Diagnostic> = semantic.into_iter().map(Into::into).collect();
        // Overlay the generated Rust into the embedded analyzer and re-publish native+rust merged from a
        // detached task: `full_diagnostics` can be slow, and notifications are awaited in order on the read
        // loop (see server.rs), so blocking here would stall completion. Native diagnostics are returned
        // now for the immediate publish; the task republishes when the analyzer is ready, and skips
        // (leaving native-only published) while it is still loading.
        if let Some(rsx_path) = file_path {
            self.spawn_rust_diagnostics(uri, rsx_path, source, theme, native.clone(), revision);
        }
        native
    }

    /// Off-loop overlay-and-merge of rust-analyzer diagnostics: maps each back onto the `.rsx` via the
    /// line map (dropping generated lines with no `.rsx` origin) and republishes native+rust. A
    /// staleness guard skips the publish if the buffer changed meanwhile, so out-of-order task
    /// completions never resurrect diagnostics for an older revision.
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
                // A newer edit already superseded this one: skip the expensive query without even
                // contending for the analyzer lock (its `full_diagnostics` would be wasted work).
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
            // The buffer moved on while the query ran → a newer revision's task will publish; don't
            // overwrite it with stale diagnostics.
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
                        ".".to_string(),
                        ":".to_string(),
                        " ".to_string(),
                        "\"".to_string(),
                    ]),
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
                color_provider: Some(ColorProviderCapability::Simple(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                code_lens_provider: Some(CodeLensOptions {
                    resolve_provider: Some(false),
                }),
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
        // Warm the embedded analyzer as soon as a `.rsx` opens, so the slow workspace
        // load overlaps with reading the file instead of stalling the first completion.
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
                    },
                );
            let theme = project.as_ref().and_then(|p| p.theme_type.clone());
            (parsed.source.clone(), native, theme)
        };

        if let Some(items) = native {
            return Some(CompletionResponse::Array(items));
        }
        // Outside a native `.rsx` zone: delegate Rust completion to the embedded rust-analyzer over
        // the generated module — line-mapped for `[logic]`, expression-span-mapped for `[view]`.
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
        Some(CompletionResponse::Array(items))
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

    /// Starts the (slow) workspace load on a blocking thread if it hasn't started yet.
    /// Returns immediately; queries that arrive while loading simply yield nothing.
    fn ensure_loading(&self, root: PathBuf, warm: Option<PathBuf>) {
        // `try_lock`, never `lock`: this runs on the single-threaded runtime, and a blocking RA query
        // can hold the mutex for the length of its `analysis` call. Blocking here would stall the whole
        // LSP read loop. Contention means the analyzer is already `Ready`/`Loading` (no query runs while
        // `Idle`), so there is nothing to start — and any state that just reset to `Idle` is picked up by
        // the next edit's call.
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
                // Compute the next state (including the slow `warm`) OUTSIDE the state lock, so queries
                // arriving mid-warm just see `Loading` instead of blocking on the mutex for ~15s.
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

    /// Maps a `[logic]` cursor into the generated module and runs `run` against the
    /// embedded analyzer on a blocking thread (the query is synchronous; the load may
    /// still be in flight, in which case this yields `None`).
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
            // A generated module the graph doesn't know yet (e.g. a `.rsx` added since
            // load): drop to Idle so the next query reloads the workspace.
            if !a.knows_file(&gen_path) {
                *state = AnalyzerState::Idle;
                return None;
            }
            let ra_at = std::time::Instant::now();
            let result = run(a, gen_path, gen_text, gen_line, gen_col);
            // Only surface slow queries — a healthy query is sub-100ms, so anything past 1s flags a
            // regression (e.g. a cold cache) without spamming the output on every keystroke.
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

    /// Maps a `[view]` cursor into the generated module via the transpiler's expression-span map and
    /// runs `run` at the resulting byte offset. Returns `None` (so native element/attr completion is
    /// preserved) when the cursor sits outside every verbatim `[view]` expression. The generated
    /// offset is a UTF-8 char boundary by construction: the fragment is byte-identical in source and
    /// output, and the `.rsx` cursor is resolved on a char boundary.
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
        let rsx_byte = rsx_byte_offset(&source, pos.line, pos.character)?;
        // The containing expression span; the inclusive upper bound lets the cursor sit right after
        // the last character (the common completion position, e.g. `count.|`).
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
        // Outside a native `.rsx` zone: resolve Rust definitions via the embedded rust-analyzer, then
        // reverse-map any generated-`.rs` targets back onto their `.rsx` (see `map_definition_targets`).
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
        // Native `.rsx` hover (tags / colors) didn't match: delegate to the embedded rust-analyzer over
        // the generated module — line-mapped for `[logic]`, expression-span-mapped for `[view]`.
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

        // Format the live buffer, not the last good parse: if the current text does not parse,
        // `format_document` returns `None` below and we emit no edit, leaving the file untouched.
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

    /// `textDocument/documentHighlight`: every occurrence of the `@class` under the cursor.
    pub async fn document_highlight(
        &self,
        params: DocumentHighlightParams,
    ) -> Option<Vec<DocumentHighlight>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let source = {
            let store = self.store.read().await;
            store.latest_source(uri).cloned()
        }?;
        let name = crate::analysis::occurrences::class_at(&source, pos.line, pos.character)?;
        let highlights = crate::analysis::occurrences::class_occurrences(&source, &name)
            .into_iter()
            .map(|range| DocumentHighlight {
                range,
                kind: Some(DocumentHighlightKind::TEXT),
            })
            .collect();
        Some(highlights)
    }

    /// `textDocument/references`: every use of the `@class` under the cursor (file-scoped symbol).
    pub async fn references(&self, params: ReferenceParams) -> Option<Vec<Location>> {
        let uri = &params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let source = {
            let store = self.store.read().await;
            store.latest_source(uri).cloned()
        }?;
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
            let locations = tokio::task::spawn_blocking(move || {
                crate::analysis::occurrences::component_references(&root, &name)
            })
            .await
            .ok()?;
            return (!locations.is_empty()).then_some(locations);
        }
        None
    }

    /// `textDocument/prepareRename`: confirm the cursor is on a renameable `@class` or signal.
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
        } else {
            return None;
        };
        Some(PrepareRenameResponse::Range(range))
    }

    /// `textDocument/rename`: rewrite every occurrence of the `@class` or signal under the cursor.
    pub async fn rename(&self, params: RenameParams) -> Option<WorkspaceEdit> {
        let uri = &params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let new_name = params.new_name;
        let source = {
            let store = self.store.read().await;
            store.latest_source(uri).cloned()
        }?;
        let occurrences = if let Some(name) =
            crate::analysis::occurrences::class_at(&source, pos.line, pos.character)
        {
            crate::analysis::occurrences::class_occurrences(&source, &name)
        } else if let Some(name) =
            crate::analysis::occurrences::signal_at(&source, pos.line, pos.character)
        {
            crate::analysis::occurrences::signal_occurrences(&source, &name)
        } else {
            return None;
        };
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
        Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        })
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
            // Prefer an `rsx.toml` root, but fall back to the Cargo workspace root so apps without an
            // `rsx.toml` (e.g. a themed app configured in code) still get workspace symbols.
            rsx_workspace::find_rsx_root(&path)
                .or_else(|| rsx_workspace::find_workspace_root(&path))?
        };
        // Reads + parses every `.rsx`, so run it off the async runtime thread.
        tokio::task::spawn_blocking(move || {
            crate::analysis::workspace_symbols::workspace_symbols(&root, &query)
        })
        .await
        .ok()
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

/// Maps rust-analyzer definition targets to `.rsx` `Location`s, handling three cases per target:
/// (1) the generated `.rs` for *this* `.rsx` and (2) *another* component's `.rsx/build/*.rs` are both
/// reverse-mapped through that build file's line map (`generated line → .rsx line`) onto its `.rsx`
/// source — a generated line with no originating `.rsx` line is dropped; (3) any other path (a
/// dependency, std, or a hand-written `.rs`) is returned verbatim in its own coordinates.
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

/// Byte offset of the `.rsx` cursor `(line, utf16_char)` within `source`, always on a UTF-8 char
/// boundary. Mirrors `ra::byte_offset` but over the `.rsx` source: the column is UTF-16 (LSP), so
/// converting it byte-wise would point mid-character on multi-byte text and yield a misaligned —
/// possibly panicking — generated offset.
fn rsx_byte_offset(source: &str, line: u32, utf16_col: u32) -> Option<usize> {
    let mut line_start = 0usize;
    for (i, current) in source.split_inclusive('\n').enumerate() {
        if i as u32 == line {
            let content = current.strip_suffix('\n').unwrap_or(current);
            let mut remaining = utf16_col;
            let mut byte = 0usize;
            for ch in content.chars() {
                let width = ch.len_utf16() as u32;
                if remaining < width {
                    break;
                }
                remaining -= width;
                byte += ch.len_utf8();
            }
            return Some(line_start + byte);
        }
        line_start += current.len();
    }
    None
}

/// Builds the range covering all of `source`, used to replace the whole document
/// with its formatted form. Character offsets are UTF-16 code units, per LSP.
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
