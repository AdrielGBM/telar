use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use lsp_types::*;
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
use crate::ra::EmbeddedAnalyzer;
use crate::rpc::OutgoingSender;
use crate::store::Store;
use crate::text::{ident_at, name_range};

use mapping::{full_document_range, leading_ws_utf16, map_definition_targets};
use telar_transpiler::nth_line;

mod lifecycle;
mod mapping;
mod query;
mod rename;

/// Lifecycle of the embedded rust-analyzer: loaded lazily on the first `[logic]` query because `load()` is slow (cargo metadata + crate graph).
// Always lives behind `Arc<Mutex<…>>` and is only ever written in place, so the large `Ready` variant is never moved by value — the size disparity clippy flags is irrelevant here.
#[allow(clippy::large_enum_variant)]
enum AnalyzerState {
    Idle,
    Loading,
    Ready(EmbeddedAnalyzer),
    Failed,
}

/// How long invalidations are coalesced before the workspace is actually torn down and reloaded. Measured from the *first* pending invalidation, never extended, so a steady stream of queries against an unknown file cannot starve the reload. Amortizes a ~15s load, so the exact value matters little.
const RELOAD_DEBOUNCE: Duration = Duration::from_millis(500);

/// Records that the crate graph is out of date without dropping the `RootDatabase`. A single `cargo add` touches `Cargo.toml` *and* `Cargo.lock`, and a branch switch touches dozens of files; tearing down per event would pay the full reload for each. Only [`Backend::ensure_loading`] acts on the mark, which keeps teardown to one place.
fn mark_reload(reload_at: &Mutex<Option<Instant>>) {
    if let Ok(mut mark) = reload_at.lock() {
        mark.get_or_insert_with(Instant::now);
    }
}

/// Whether `path` sits inside a cargo build directory. `ra::load` sets `load_out_dirs_from_check`, so every workspace load runs `cargo check` and writes generated `.rs` under `target/`; treating those as source changes would reload the workspace in an endless loop. The client cannot be trusted to exclude them — a non-VS Code editor registers its own watchers.
fn is_under_target_dir(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == "target")
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
    reload_at: Arc<Mutex<Option<Instant>>>,
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
            reload_at: Arc::new(Mutex::new(None)),
        }
    }

    pub fn outgoing(&self) -> &OutgoingSender {
        &self.outgoing
    }

    /// Drops the embedded analyzer, and with it the proc-macro server child, while the process is still alive. `server.rs` cannot rely on `drop(backend)` for this: spawned request handlers hold their own `Arc<Backend>` clones, so the `Drop` that kills the child may not run before the process exits, leaving it reparented to init holding its share of a multi-GB database.
    pub fn release_analyzer(&self) {
        if let Ok(mut state) = self.analyzer.lock() {
            *state = AnalyzerState::Idle;
        }
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
            .log_message(MessageType::INFO, "telar-analyzer initialized");
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
            if crate::build_sync::is_generated_build_file(&path) || is_under_target_dir(&path) {
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

        if needs_reload {
            mark_reload(&self.reload_at);
            self.outgoing.log_message(
                MessageType::INFO,
                "telar-analyzer: manifest/file change — workspace reload queued".to_string(),
            );
            return;
        }
        if to_refresh.is_empty() {
            return;
        }
        let analyzer = self.analyzer.clone();
        let reload_at = self.reload_at.clone();
        // Off the read loop: locking the analyzer can contend with an in-flight RA query.
        tokio::task::spawn_blocking(move || {
            let Ok(mut state) = analyzer.lock() else {
                return;
            };
            if let AnalyzerState::Ready(a) = &mut *state {
                for path in &to_refresh {
                    if !a.refresh_from_disk(path) {
                        // A `.rs` the loaded graph doesn't know (e.g. newly created) → reload to pick it up.
                        mark_reload(&reload_at);
                        break;
                    }
                }
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
                        // The whole workspace, not the file's own directory: a component lives wherever its crate is, and offering only its siblings is what keeps `.rsx` files from composing.
                        CompletionKind::ElementName => element_name_items(
                            project
                                .as_ref()
                                .map(|p| p.component_root.as_path())
                                .or_else(|| file_path.as_deref().and_then(|p| p.parent())),
                        ),
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
        let items = self
            .rust_query(rsx_path, source, theme, pos, |a, path, text, offset| {
                Some(a.completions_at_offset(&path, text, offset))
            })
            .await?;
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
        self.rust_query(rsx_path, source, theme, pos, |a, path, text, offset| {
            a.signature_help_at_offset(&path, text, offset)
        })
        .await
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
        let targets = self
            .rust_query(rsx_path, source, theme, pos, |a, path, text, offset| {
                a.definition_at_offset(&path, text, offset)
            })
            .await?;
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
        self.rust_query(rsx_path, source, theme, pos, |a, path, text, offset| {
            a.hover_at_offset(&path, text, offset)
        })
        .await
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
        let path = crate::uri::to_path(uri)?;
        // Resolve links against the project asset root (matching the baker), falling back to the file's dir when there is no telar.toml.
        let assets_dir = telar_transpiler::find_telar_root(&path)
            .map(|root| telar_transpiler::assets_root(&root))
            .or_else(|| path.parent().map(|p| p.to_path_buf()))?;
        let store = self.store.read().await;
        let parsed = store.get(uri)?;
        Some(crate::analysis::links::document_links(
            &parsed.document,
            &parsed.source,
            &assets_dir,
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
            let Some(Some(rsx_line)) = target.map.lines.get(raw.line as usize) else {
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
            // Workspace first: a component is referenced from wherever it is used, which in a multi-crate project is not the crate that defines it. The nearest `telar.toml` is the narrower answer and is only right when there is no workspace above it.
            let root = telar_transpiler::find_workspace_root(&path)
                .or_else(|| telar_transpiler::find_telar_root(&path))?;
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
                        "telar-analyzer: rename skipped — {unmapped} reference(s) couldn't be precisely located (non-verbatim [view] usage or cross-component). Rename left unchanged."
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
            // The Cargo workspace root, so a `workspace/symbol` query answers for the whole workspace rather than for whichever crate happened to have a file open; the nearest `telar.toml` is the fallback for a project that is not in one.
            telar_transpiler::find_workspace_root(&path)
                .or_else(|| telar_transpiler::find_telar_root(&path))?
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
