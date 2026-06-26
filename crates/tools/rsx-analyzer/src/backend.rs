use std::sync::Arc;

use lsp_types::*;
use rsx_diagnostics::semantic_diagnostics;
use tokio::sync::RwLock;

use crate::analysis::completions::{
    CompletionKind, attribute_key_items, color_items, completion_context, element_name_items,
    style_class_items,
};
use crate::analysis::definition::goto_definition;
use crate::analysis::hover::hover_info;
use crate::project::ProjectInfo;
use crate::rpc::OutgoingSender;
use crate::store::Store;

pub struct Backend {
    outgoing: OutgoingSender,
    store: Arc<RwLock<Store>>,
}

impl Backend {
    pub fn new(outgoing: OutgoingSender) -> Self {
        Self {
            outgoing,
            store: Arc::new(RwLock::new(Store::new())),
        }
    }

    pub fn outgoing(&self) -> &OutgoingSender {
        &self.outgoing
    }

    async fn reparse_and_diagnose(&self, uri: Uri, text: String) -> Vec<Diagnostic> {
        let file_path = crate::uri::to_path(&uri);
        let mut store = self.store.write().await;
        let parse_diagnostics = store.reparse(uri.clone(), text);
        if !parse_diagnostics.is_empty() {
            return parse_diagnostics.into_iter().map(Into::into).collect();
        }
        let semantic = store
            .get(&uri)
            .map(|parsed| {
                let project = file_path.as_deref().and_then(ProjectInfo::discover);
                let theme_view = project.as_ref().map(ProjectInfo::theme_view);
                let diagnostics = semantic_diagnostics(&parsed.document, theme_view.as_ref());
                // Mirror the live buffer to its generated `.rs` so the workspace rust-analyzer analyzes
                // the in-flight text — this is what makes completion/hover/definition live instead of one
                // `cargo check` behind. Same output as the `app!` macro produces at compile time.
                if let Some(rsx_path) = file_path.as_deref() {
                    let theme = project.as_ref().and_then(|p| p.theme_type.as_deref());
                    crate::build_sync::sync_build_file(rsx_path, &parsed.source, theme);
                }
                diagnostics
            })
            .unwrap_or_default();
        semantic.into_iter().map(Into::into).collect()
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
        let diagnostics = self.reparse_and_diagnose(uri.clone(), text).await;
        self.outgoing.publish_diagnostics(uri, diagnostics);
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
        let file_path = crate::uri::to_path(uri);

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
        let file_path = crate::uri::to_path(uri);

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
