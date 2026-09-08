//! The queries the backend delegates to rust-analyzer: completion, signature help, hover, definition, references, diagnostics and inlay hints.
//!
//! Every one of them syncs the live transpile first, so the answer is against the buffer the editor holds rather than whatever was last built. Offsets come in as bytes because that is what the transpiler's source map speaks; they are converted at this boundary, since LSP counts UTF-16 units from a line.

use std::path::{Path, PathBuf};

use lsp_types::{
    CompletionItem, Diagnostic, Hover, InlayHint, InlayHintLabel, Location, Position, Range,
    SignatureHelp,
};
use serde_json::{Value, json};

use super::{Analyzer, DefinitionTarget, InlayHintRaw, QUERY_TIMEOUT, RefTarget};

impl Analyzer {
    pub async fn completions(
        &self,
        gen_path: &Path,
        generated: &str,
        offset: usize,
    ) -> Vec<CompletionItem> {
        let Some(result) = self
            .at_offset("textDocument/completion", gen_path, generated, offset, None)
            .await
        else {
            return Vec::new();
        };
        // A `CompletionList` when rust-analyzer flags incompleteness, a bare array otherwise.
        let items = result.get("items").cloned().unwrap_or(result);
        serde_json::from_value(items).unwrap_or_default()
    }

    pub async fn signature_help(
        &self,
        gen_path: &Path,
        generated: &str,
        offset: usize,
    ) -> Option<SignatureHelp> {
        let result = self
            .at_offset(
                "textDocument/signatureHelp",
                gen_path,
                generated,
                offset,
                None,
            )
            .await?;
        serde_json::from_value(result).ok()
    }

    /// The range is dropped so the client highlights the hovered `.rsx` word itself — reverse-mapping a generated-file range is pointless for a tooltip.
    pub async fn hover(&self, gen_path: &Path, generated: &str, offset: usize) -> Option<Hover> {
        let result = self
            .at_offset("textDocument/hover", gen_path, generated, offset, None)
            .await?;
        let mut hover: Hover = serde_json::from_value(result).ok()?;
        hover.range = None;
        Some(hover)
    }

    pub async fn definition(
        &self,
        gen_path: &Path,
        generated: &str,
        offset: usize,
    ) -> Option<Vec<DefinitionTarget>> {
        let result = self
            .at_offset("textDocument/definition", gen_path, generated, offset, None)
            .await?;
        Some(
            locations(result)
                .into_iter()
                .map(|(path, range)| DefinitionTarget { path, range })
                .collect(),
        )
    }

    /// Find-all-references, declaration included: a component rename needs the definition site as much as the uses.
    pub async fn references(
        &self,
        gen_path: &Path,
        generated: &str,
        offset: usize,
    ) -> Option<Vec<RefTarget>> {
        let result = self
            .at_offset(
                "textDocument/references",
                gen_path,
                generated,
                offset,
                Some(json!({ "context": { "includeDeclaration": true } })),
            )
            .await?;
        Some(
            locations(result)
                .into_iter()
                .map(|(path, range)| RefTarget { path, range })
                .collect(),
        )
    }

    /// Diagnostics for the generated file, in generated-file coordinates. Pulled rather than taken from the `publishDiagnostics` rust-analyzer also emits, so a stale push cannot be mistaken for the current buffer's answer.
    pub async fn diagnostics(&self, gen_path: &Path, generated: &str) -> Vec<Diagnostic> {
        self.inner.sync(gen_path, generated);
        let result = self
            .inner
            .request(
                "textDocument/diagnostic",
                json!({ "textDocument": { "uri": crate::inner::uri_for(gen_path) } }),
                QUERY_TIMEOUT,
            )
            .await;
        let Some(result) = result else {
            return Vec::new();
        };
        serde_json::from_value(result.get("items").cloned().unwrap_or_default()).unwrap_or_default()
    }

    /// Type and parameter hints over the whole generated file. The backend keeps only those whose line maps back to `[logic]`.
    pub async fn inlay_hints(&self, gen_path: &Path, generated: &str) -> Vec<InlayHintRaw> {
        self.inner.sync(gen_path, generated);
        let result = self
            .inner
            .request(
                "textDocument/inlayHint",
                json!({
                    "textDocument": { "uri": crate::inner::uri_for(gen_path) },
                    "range": whole_of(generated),
                }),
                QUERY_TIMEOUT,
            )
            .await;
        let Some(result) = result else {
            return Vec::new();
        };
        let hints: Vec<InlayHint> = serde_json::from_value(result).unwrap_or_default();
        hints
            .into_iter()
            .filter_map(|hint| {
                let label = match hint.label {
                    InlayHintLabel::String(label) => label,
                    InlayHintLabel::LabelParts(parts) => {
                        parts.into_iter().map(|p| p.value).collect()
                    }
                };
                if label.is_empty() {
                    return None;
                }
                Some(InlayHintRaw {
                    line: hint.position.line,
                    col: hint.position.character,
                    pad_left: hint.padding_left.unwrap_or(false),
                    pad_right: hint.padding_right.unwrap_or(false),
                    kind: hint.kind,
                    label,
                })
            })
            .collect()
    }

    async fn at_offset(
        &self,
        method: &str,
        gen_path: &Path,
        generated: &str,
        offset: usize,
        extra: Option<Value>,
    ) -> Option<Value> {
        self.inner.sync(gen_path, generated);
        let mut params = json!({
            "textDocument": { "uri": crate::inner::uri_for(gen_path) },
            "position": crate::text::offset_to_position(generated, offset),
        });
        if let (Some(Value::Object(extra)), Some(params)) = (extra, params.as_object_mut()) {
            params.extend(extra);
        }
        let result = self.inner.request(method, params, QUERY_TIMEOUT).await?;
        (!result.is_null()).then_some(result)
    }
}

/// LSP's `Location | Location[]` as `(path, range)` pairs. Anything whose URI is not a local file is dropped, since the backend has nothing to map it onto.
fn locations(result: Value) -> Vec<(PathBuf, Range)> {
    let list = match result {
        Value::Array(list) => list,
        other => vec![other],
    };
    list.into_iter()
        .filter_map(|value| serde_json::from_value::<Location>(value).ok())
        .filter_map(|location| Some((crate::uri::to_path(&location.uri)?, location.range)))
        .collect()
}

fn whole_of(text: &str) -> Range {
    Range {
        start: Position {
            line: 0,
            character: 0,
        },
        end: crate::text::offset_to_position(text, text.len()),
    }
}
