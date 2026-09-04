//! The analyzer queries the backend delegates to: completion, hover, definition, references and rename.

use std::path::Path;

use lsp_types::{
    CompletionItem, Hover, HoverContents, MarkupContent, MarkupKind, ParameterInformation,
    ParameterLabel, Range, SignatureHelp, SignatureInformation,
};
use ra_ap_ide::{Analysis, FilePosition, FileRange, TextRange, TextSize};
use ra_ap_vfs::FileId;

use super::config::{
    completion_config, find_all_refs_config, goto_definition_config, hover_config,
};
use super::mapping::{lsp_position, map_completion_kind, map_documentation};
use super::{DefinitionTarget, EmbeddedAnalyzer, RefTarget};

impl EmbeddedAnalyzer {
    /// Completion at an exact byte `offset` in the generated file: overlays the freshly transpiled Rust for `gen_path`, then queries rust-analyzer there. The backend resolves the `.rsx` cursor to that offset. `offset` must land on a UTF-8 char boundary or rust-analyzer panics.
    pub fn completions_at_offset(
        &mut self,
        gen_path: &Path,
        generated: String,
        offset: TextSize,
    ) -> Vec<CompletionItem> {
        let Some(file_id) = self.file_id(gen_path) else {
            return Vec::new();
        };
        self.overlay(file_id, generated);
        let analysis = self.host.analysis();
        let config = completion_config();
        let pos = FilePosition { file_id, offset };
        let items = analysis
            .completions(&config, pos, None)
            .ok()
            .flatten()
            .unwrap_or_default();
        items
            .into_iter()
            .map(|item| CompletionItem {
                label: item.lookup().to_string(),
                kind: map_completion_kind(item.kind),
                detail: item.detail.clone(),
                // Eager from rust-analyzer (the completion config resolves nothing lazily). The backend moves this into its resolve cache and re-attaches it on `completionItem/resolve`, so the completion list itself stays lean on the wire.
                documentation: map_documentation(&item),
                ..Default::default()
            })
            .collect()
    }

    /// Signature help at an exact byte `offset` in the generated file, mapped to LSP. Same overlay→query path as completion.
    pub fn signature_help_at_offset(
        &mut self,
        gen_path: &Path,
        generated: String,
        offset: TextSize,
    ) -> Option<SignatureHelp> {
        let file_id = self.file_id(gen_path)?;
        self.overlay(file_id, generated);
        let analysis = self.host.analysis();
        let pos = FilePosition { file_id, offset };
        let help = analysis.signature_help(pos).ok().flatten()?;
        let active = help.active_parameter.map(|n| n as u32);
        let parameters = help
            .parameter_labels()
            .map(|p| ParameterInformation {
                label: ParameterLabel::Simple(p.to_string()),
                documentation: None,
            })
            .collect();
        Some(SignatureHelp {
            signatures: vec![SignatureInformation {
                label: help.signature.clone(),
                documentation: None,
                parameters: Some(parameters),
                active_parameter: active,
            }],
            active_signature: Some(0),
            active_parameter: active,
        })
    }

    /// Hover at an exact byte `offset` in the generated file, mapped to LSP. The range is omitted so the client highlights the hovered `.rsx` word itself — mapping the generated-file range back is unnecessary for a tooltip.
    pub fn hover_at_offset(
        &mut self,
        gen_path: &Path,
        generated: String,
        offset: TextSize,
    ) -> Option<Hover> {
        let file_id = self.file_id(gen_path)?;
        self.overlay(file_id, generated);
        let analysis = self.host.analysis();
        let range = FileRange {
            file_id,
            range: TextRange::empty(offset),
        };
        let info = analysis.hover(&hover_config(), range).ok().flatten()?;
        let value = info.info.markup.as_str().to_string();
        if value.is_empty() {
            return None;
        }
        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value,
            }),
            range: None,
        })
    }

    /// Go-to-definition at an exact byte `offset` in the generated file. Each navigation target is resolved to its file path (via the `Vfs`) and name range (in that file's coordinates); the backend reverse-maps the generated `.telar/build/*.rs` ones back to the `.rsx`. `None` means rust-analyzer found nothing.
    pub fn definition_at_offset(
        &mut self,
        gen_path: &Path,
        generated: String,
        offset: TextSize,
    ) -> Option<Vec<DefinitionTarget>> {
        let file_id = self.file_id(gen_path)?;
        self.overlay(file_id, generated);
        let analysis = self.host.analysis();
        let pos = FilePosition { file_id, offset };
        let nav = analysis
            .goto_definition(pos, &goto_definition_config())
            .ok()
            .flatten()?;
        let mut targets = Vec::new();
        for target in nav.info {
            let Some(path) = self.file_path(target.file_id) else {
                continue;
            };
            let Ok(line_index) = analysis.file_line_index(target.file_id) else {
                continue;
            };
            // Prefer the identifier range (`focus_range`); fall back to the whole item.
            let span = target.focus_range.unwrap_or(target.full_range);
            targets.push(DefinitionTarget {
                path,
                range: Range {
                    start: lsp_position(&line_index, span.start()),
                    end: lsp_position(&line_index, span.end()),
                },
            });
        }
        Some(targets)
    }

    /// Find-all-references at an exact byte `offset` in the generated file (component rename queries the generated `fn`/`Props` definition directly). Returns the declaration plus every use across the workspace; `None` when rust-analyzer resolves no symbol.
    pub fn references_at_offset(
        &mut self,
        gen_path: &Path,
        generated: String,
        offset: TextSize,
    ) -> Option<Vec<RefTarget>> {
        let file_id = self.file_id(gen_path)?;
        self.overlay(file_id, generated);
        let analysis = self.host.analysis();
        let pos = FilePosition { file_id, offset };
        let results = analysis
            .find_all_refs(pos, &find_all_refs_config())
            .ok()
            .flatten()?;
        let mut out = Vec::new();
        for result in &results {
            if let Some(decl) = &result.declaration {
                let nav = &decl.nav;
                let span = nav.focus_range.unwrap_or(nav.full_range);
                if let Some(target) = self.ref_target(&analysis, nav.file_id, span) {
                    out.push(target);
                }
            }
            for (file_id, ranges) in &result.references {
                for (range, _category) in ranges {
                    if let Some(target) = self.ref_target(&analysis, *file_id, *range) {
                        out.push(target);
                    }
                }
            }
        }
        Some(out)
    }

    /// Builds a [`RefTarget`] for `span` in `file_id`: its filesystem path (via the `Vfs`) plus the span in both byte and LSP coordinates. `None` if the file has no path or no line index.
    fn ref_target(
        &self,
        analysis: &Analysis,
        file_id: FileId,
        span: TextRange,
    ) -> Option<RefTarget> {
        let path = self.file_path(file_id)?;
        let line_index = analysis.file_line_index(file_id).ok()?;
        Some(RefTarget {
            path,
            byte_start: span.start().into(),
            byte_end: span.end().into(),
            range: Range {
                start: lsp_position(&line_index, span.start()),
                end: lsp_position(&line_index, span.end()),
            },
        })
    }
}
