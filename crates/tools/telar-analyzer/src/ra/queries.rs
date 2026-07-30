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
use super::mapping::{byte_offset, lsp_position, map_completion_kind, map_documentation};
use super::{DefinitionTarget, EmbeddedAnalyzer, RefTarget};

impl EmbeddedAnalyzer {
    /// `[logic]` completion: overlays the freshly transpiled Rust for `gen_path`, then queries rust-analyzer at the mapped cursor. `line`/`col` are in the generated file (`.rsx` line resolved through the source map, column already `+LOGIC_INDENT`).
    pub fn completions_at(
        &mut self,
        gen_path: &Path,
        generated: String,
        line: u32,
        col: u32,
    ) -> Vec<CompletionItem> {
        let Some(offset) = byte_offset(&generated, line, col) else {
            return Vec::new();
        };
        self.completions_at_offset(gen_path, generated, offset)
    }

    /// Completion at an exact byte `offset` in the generated file. The `[view]` path computes the offset itself (via the expression-span map), so it bypasses the line/column mapping that the `[logic]` path uses. `offset` must land on a UTF-8 char boundary or rust-analyzer panics.
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

    /// `[logic]` signature help, mapped to LSP. Same overlay→query path as completion.
    pub fn signature_help_at(
        &mut self,
        gen_path: &Path,
        generated: String,
        line: u32,
        col: u32,
    ) -> Option<SignatureHelp> {
        let offset = byte_offset(&generated, line, col)?;
        self.signature_help_at_offset(gen_path, generated, offset)
    }

    /// Signature help at an exact byte `offset` in the generated file (used by the `[view]` path). `offset` must land on a UTF-8 char boundary.
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

    /// `[logic]` hover, mapped to LSP. Same overlay→query path as completion.
    pub fn hover_at(
        &mut self,
        gen_path: &Path,
        generated: String,
        line: u32,
        col: u32,
    ) -> Option<Hover> {
        let offset = byte_offset(&generated, line, col)?;
        self.hover_at_offset(gen_path, generated, offset)
    }

    /// Hover at an exact byte `offset` in the generated file (used by the `[view]` path). `offset` must land on a UTF-8 char boundary. The range is omitted so the client highlights the hovered `.rsx` word itself — mapping the generated-file range back is unnecessary for a tooltip.
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

    /// `[logic]` go-to-definition. Same overlay→query path as completion.
    pub fn definition_at(
        &mut self,
        gen_path: &Path,
        generated: String,
        line: u32,
        col: u32,
    ) -> Option<Vec<DefinitionTarget>> {
        let offset = byte_offset(&generated, line, col)?;
        self.definition_at_offset(gen_path, generated, offset)
    }

    /// Definition at an exact byte `offset` in the generated file (used by the `[view]` path). `offset` must land on a UTF-8 char boundary. Each navigation target is resolved to its file path (via the `Vfs`) and name range (in that file's coordinates); the backend reverse-maps the generated `.telar/build/*.rs` ones back to the `.rsx`. `None` means rust-analyzer found nothing.
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

    /// `[logic]` find-all-references. Same overlay→query path as completion; the cursor is mapped through the line map by the caller (`line`/`col` are generated-file coordinates).
    pub fn references_at(
        &mut self,
        gen_path: &Path,
        generated: String,
        line: u32,
        col: u32,
    ) -> Option<Vec<RefTarget>> {
        let offset = byte_offset(&generated, line, col)?;
        self.references_at_offset(gen_path, generated, offset)
    }

    /// Find-all-references at an exact byte `offset` in the generated file (used by the `[view]` path and by component rename, which queries the generated `fn`/`Props` definition directly). Returns the declaration plus every use across the workspace; `None` when rust-analyzer resolves no symbol.
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
