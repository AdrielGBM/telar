use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use lsp_types::*;
use tokio::sync::{Mutex, RwLock};

use crate::analysis::completions::{
    CompletionKind, attribute_key_items, color_items, completion_context, element_name_items,
    style_class_items,
};
use crate::analysis::definition::goto_definition;
use crate::analysis::diagnostics::semantic_diagnostics;
use crate::analysis::hover::hover_info;
use crate::logic_sync::{ensure_cargo_toml, logic_file_path, lsp_dir, remove_logic_file};
use crate::position::{Section, find_section_at, rs_to_rsx_line, rsx_to_rs_line};
use crate::project::ProjectInfo;
use crate::ra_client::RaClient;
use crate::rpc::OutgoingSender;
use crate::store::Store;

pub struct Backend {
    outgoing: OutgoingSender,
    store: Arc<RwLock<Store>>,
    ra_client: Arc<Mutex<Option<RaClient>>>,
    current_root: Arc<Mutex<Option<std::path::PathBuf>>>,
    semantic_tokens_registered: Arc<AtomicBool>,
}

impl Backend {
    pub fn new(outgoing: OutgoingSender) -> Self {
        Self {
            outgoing,
            store: Arc::new(RwLock::new(Store::new())),
            ra_client: Arc::new(Mutex::new(None)),
            current_root: Arc::new(Mutex::new(None)),
            semantic_tokens_registered: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn outgoing(&self) -> &OutgoingSender {
        &self.outgoing
    }

    async fn reparse_and_diagnose(&self, uri: Uri, text: String, is_open: bool) -> Vec<Diagnostic> {
        let file_path = crate::uri::to_path(&uri);
        let mut store = self.store.write().await;
        let parse_diagnostics = store.reparse(uri.clone(), text);
        if !parse_diagnostics.is_empty() {
            return parse_diagnostics;
        }
        let (semantic, logic_source, project) = store
            .get(&uri)
            .map(|parsed| {
                let discovered_project = file_path.as_deref().and_then(ProjectInfo::discover);
                let diagnostics =
                    semantic_diagnostics(&parsed.document, discovered_project.as_ref());
                let logic = parsed.document.logic.source.clone();
                (diagnostics, Some(logic), discovered_project)
            })
            .unwrap_or_default();
        drop(store);

        if let (Some(logic_src), Some(project), Some(rsx_path)) =
            (&logic_source, &project, file_path.as_deref())
        {
            sync_logic_zone_from_str(logic_src, rsx_path, &project.root);
            let mut ra = self.ra_client.lock().await;
            let mut root_guard = self.current_root.lock().await;
            if ra.is_none() || root_guard.as_deref() != Some(&project.root) {
                *ra = RaClient::spawn(&lsp_dir(&project.root)).await;
                *root_guard = Some(project.root.clone());

                if !self.semantic_tokens_registered.load(Ordering::SeqCst) {
                    if let Some(ra_ref) = ra.as_ref() {
                        let legend = ra_ref.legend().clone();
                        let register_options = serde_json::json!({
                            "documentSelector": [{ "language": "rsx" }],
                            "legend": {
                                "tokenTypes": legend.token_types.iter().map(|t| t.as_str()).collect::<Vec<_>>(),
                                "tokenModifiers": legend.token_modifiers.iter().map(|m| m.as_str()).collect::<Vec<_>>()
                            },
                            "full": true
                        });
                        self.outgoing.register_capability(vec![Registration {
                            id: "rsx-semantic-tokens".to_string(),
                            method: "textDocument/semanticTokens".to_string(),
                            register_options: Some(register_options),
                        }]);
                        self.semantic_tokens_registered
                            .store(true, Ordering::SeqCst);
                    }
                }
            }
            drop(root_guard);
            if let (Some(ra_ref), Some(logic_file_path_buf)) =
                (ra.as_ref(), logic_file_path(&project.root, rsx_path))
            {
                if let Some(logic_file_uri) = crate::uri::from_path(&logic_file_path_buf) {
                    if is_open {
                        ra_ref.did_open(&logic_file_uri, logic_src).await;
                    } else {
                        ra_ref.did_change(&logic_file_uri, logic_src, 1).await;
                    }
                }
            }
        }

        semantic
    }

    pub fn initialize(&self) -> InitializeResult {
        InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        ".".to_string(),
                        ":".to_string(),
                        " ".to_string(),
                        "\"".to_string(),
                    ]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
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
        let diagnostics = self.reparse_and_diagnose(uri.clone(), text, true).await;
        self.outgoing.publish_diagnostics(uri, diagnostics);
    }

    pub async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        if let Some(change) = params.content_changes.into_iter().last() {
            let diagnostics = self
                .reparse_and_diagnose(uri.clone(), change.text, false)
                .await;
            self.outgoing.publish_diagnostics(uri, diagnostics);
        }
    }

    pub async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let file_path = crate::uri::to_path(&uri);
        let project = file_path.as_deref().and_then(ProjectInfo::discover);

        if let (Some(project), Some(rsx_path)) = (project.as_ref(), file_path.as_deref()) {
            if let Some(logic_file_path_buf) = logic_file_path(&project.root, rsx_path) {
                if let Some(logic_file_uri) = crate::uri::from_path(&logic_file_path_buf) {
                    if let Some(ra) = self.ra_client.lock().await.as_ref() {
                        ra.did_close(&logic_file_uri).await;
                    }
                }
            }
            remove_logic_file(rsx_path, &project.root);
        }

        self.store.write().await.close(&uri);
    }

    pub async fn completion(&self, params: CompletionParams) -> Option<CompletionResponse> {
        let uri = &params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let file_path = crate::uri::to_path(&uri);

        // Check section (briefly hold read lock)
        let logic_rs_line = {
            let store = self.store.read().await;
            let parsed = store.get(uri)?;
            if find_section_at(&parsed.source, pos.line) == Section::Logic {
                rsx_to_rs_line(&parsed.source, pos.line)
            } else {
                None
            }
        };

        if let Some(rs_line) = logic_rs_line {
            let project = file_path.as_deref().and_then(ProjectInfo::discover);
            let ra = self.ra_client.lock().await;
            if let (Some(ra_ref), Some(project), Some(rsx_path)) =
                (ra.as_ref(), project.as_ref(), file_path.as_deref())
            {
                if let Some(logic_file_uri) =
                    logic_file_path(&project.root, rsx_path).and_then(|p| crate::uri::from_path(&p))
                {
                    let items = ra_ref
                        .completion(&logic_file_uri, rs_line, pos.character)
                        .await;
                    return Some(CompletionResponse::Array(items));
                }
            }
            return None;
        }

        let store = self.store.read().await;
        let parsed = store.get(uri)?;
        let kind = completion_context(&parsed.source, pos.line, pos.character)?;
        let project = file_path.as_deref().and_then(ProjectInfo::discover);
        let items = match kind {
            CompletionKind::ElementName => {
                element_name_items(file_path.as_deref().and_then(|p| p.parent()))
            }
            CompletionKind::AttributeKey(tag) => attribute_key_items(&tag),
            CompletionKind::ColorValue => color_items(&parsed.document, project.as_ref()),
            CompletionKind::StyleClass => style_class_items(&parsed.document),
        };
        Some(CompletionResponse::Array(items))
    }

    pub async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Option<GotoDefinitionResponse> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let file_path = crate::uri::to_path(&uri);

        let (logic_rs_line, source_clone) = {
            let store = self.store.read().await;
            let parsed = store.get(uri)?;
            if find_section_at(&parsed.source, pos.line) == Section::Logic {
                let rs_line = rsx_to_rs_line(&parsed.source, pos.line);
                (rs_line, Some(parsed.source.clone()))
            } else {
                (None, None)
            }
        };

        if let (Some(rs_line), Some(src)) = (logic_rs_line, source_clone) {
            let project = file_path.as_deref().and_then(ProjectInfo::discover);
            let ra = self.ra_client.lock().await;
            if let (Some(ra_ref), Some(project), Some(rsx_path)) =
                (ra.as_ref(), project.as_ref(), file_path.as_deref())
            {
                if let Some(logic_file_uri) =
                    logic_file_path(&project.root, rsx_path).and_then(|p| crate::uri::from_path(&p))
                {
                    if let Some(resp) = ra_ref
                        .definition(&logic_file_uri, rs_line, pos.character)
                        .await
                    {
                        return Some(remap_definition_to_rsx(resp, uri, &src));
                    }
                }
            }
            return None;
        }

        let store = self.store.read().await;
        let parsed = store.get(uri)?;
        let project = file_path.as_deref().and_then(ProjectInfo::discover);
        goto_definition(
            &parsed.document,
            &parsed.source,
            uri,
            pos.line,
            pos.character,
            project.as_ref(),
        )
    }

    pub async fn hover(&self, params: HoverParams) -> Option<Hover> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let file_path = crate::uri::to_path(&uri);

        let logic_rs_line = {
            let store = self.store.read().await;
            let parsed = store.get(uri)?;
            if find_section_at(&parsed.source, pos.line) == Section::Logic {
                rsx_to_rs_line(&parsed.source, pos.line)
            } else {
                None
            }
        };

        if let Some(rs_line) = logic_rs_line {
            let project = file_path.as_deref().and_then(ProjectInfo::discover);
            let ra = self.ra_client.lock().await;
            if let (Some(ra_ref), Some(project), Some(rsx_path)) =
                (ra.as_ref(), project.as_ref(), file_path.as_deref())
            {
                if let Some(logic_file_uri) =
                    logic_file_path(&project.root, rsx_path).and_then(|p| crate::uri::from_path(&p))
                {
                    return ra_ref.hover(&logic_file_uri, rs_line, pos.character).await;
                }
            }
            return None;
        }

        let store = self.store.read().await;
        let parsed = store.get(uri)?;
        let project = file_path.as_deref().and_then(ProjectInfo::discover);
        hover_info(
            &parsed.document,
            &parsed.source,
            pos.line,
            pos.character,
            project.as_ref(),
        )
    }

    pub async fn formatting(&self, params: DocumentFormattingParams) -> Option<Vec<TextEdit>> {
        let uri = &params.text_document.uri;

        let source = {
            let store = self.store.read().await;
            store.get(uri).map(|parsed| parsed.source.clone())
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

    pub async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Option<SemanticTokensResult> {
        let uri = &params.text_document.uri;
        let file_path = crate::uri::to_path(&uri);

        let source = {
            let store = self.store.read().await;
            store.get(uri).map(|p| p.source.clone())
        }?;

        let project = file_path.as_deref().and_then(ProjectInfo::discover);
        let ra = self.ra_client.lock().await;

        if let (Some(ra_ref), Some(project), Some(rsx_path)) =
            (ra.as_ref(), project.as_ref(), file_path.as_deref())
        {
            if let Some(logic_file_uri) =
                logic_file_path(&project.root, rsx_path).and_then(|p| crate::uri::from_path(&p))
            {
                if let Some(raw_data) = ra_ref.semantic_tokens_full(&logic_file_uri).await {
                    let decoded = crate::semantic_tokens::decode_tokens(&raw_data);
                    let remapped: Vec<(u32, u32, u32, u32, u32)> = decoded
                        .into_iter()
                        .map(|(rs_line, character, len, token_type, token_modifiers)| {
                            let rsx_line = crate::position::rs_to_rsx_line(&source, rs_line);
                            (rsx_line, character, len, token_type, token_modifiers)
                        })
                        .collect();
                    let data = crate::semantic_tokens::encode_tokens(&remapped);
                    return Some(SemanticTokensResult::Tokens(SemanticTokens {
                        result_id: None,
                        data,
                    }));
                }
            }
        }
        None
    }
}

fn sync_logic_zone_from_str(
    logic_source: &str,
    rsx_path: &std::path::Path,
    project_root: &std::path::Path,
) {
    let dir = lsp_dir(project_root);
    let _ = std::fs::create_dir_all(&dir);
    ensure_cargo_toml(&dir, project_root);
    if let Some(logic_path) = logic_file_path(project_root, rsx_path) {
        if let Some(parent) = logic_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(logic_path, logic_source);
    }
}

/// Remaps a goto-definition response from `.rsx/lsp/<stem>.rs` coordinates back to `.rsx` coordinates.
fn remap_definition_to_rsx(
    resp: GotoDefinitionResponse,
    rsx_uri: &Uri,
    rsx_source: &str,
) -> GotoDefinitionResponse {
    match resp {
        GotoDefinitionResponse::Scalar(loc) => {
            GotoDefinitionResponse::Scalar(remap_location(loc, rsx_uri, rsx_source))
        }
        GotoDefinitionResponse::Array(locs) => GotoDefinitionResponse::Array(
            locs.into_iter()
                .map(|l| remap_location(l, rsx_uri, rsx_source))
                .collect(),
        ),
        other => other,
    }
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

fn remap_location(loc: Location, rsx_uri: &Uri, rsx_source: &str) -> Location {
    let rsx_line = rs_to_rsx_line(rsx_source, loc.range.start.line);
    Location {
        uri: rsx_uri.clone(),
        range: Range {
            start: Position {
                line: rsx_line,
                character: loc.range.start.character,
            },
            end: Position {
                line: rs_to_rsx_line(rsx_source, loc.range.end.line),
                character: loc.range.end.character,
            },
        },
    }
}
