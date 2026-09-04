//! Pulling diagnostics out of the embedded analyzer for one generated file.

use std::path::Path;

use lsp_types::{Diagnostic, InlayHintKind, NumberOrString, Range};
use ra_ap_ide::{AssistResolveStrategy, InlayHintPosition, InlayKind};

use super::config::{diagnostics_config, inlay_hints_config};
use super::mapping::{lsp_position, map_severity};
use super::{EmbeddedAnalyzer, InlayHintRaw};

impl EmbeddedAnalyzer {
    /// rust-analyzer diagnostics for the overlaid generated file, in generated-file coordinates. The backend reverse-maps each line back onto the `.rsx` via the source map. Only diagnostics whose range is in the generated file are kept (a cross-file diagnostic has no `.rsx` line to map to).
    pub fn diagnostics(&mut self, gen_path: &Path, generated: String) -> Vec<Diagnostic> {
        let Some(file_id) = self.file_id(gen_path) else {
            return Vec::new();
        };
        self.overlay(file_id, generated);
        let analysis = self.host.analysis();
        let raw = analysis
            .full_diagnostics(&diagnostics_config(), AssistResolveStrategy::None, file_id)
            .unwrap_or_default();
        let Ok(line_index) = analysis.file_line_index(file_id) else {
            return Vec::new();
        };
        raw.into_iter()
            .filter(|d| d.range.file_id == file_id)
            .filter_map(|d| {
                let severity = map_severity(d.severity)?;
                Some(Diagnostic {
                    range: Range {
                        start: lsp_position(&line_index, d.range.range.start()),
                        end: lsp_position(&line_index, d.range.range.end()),
                    },
                    severity: Some(severity),
                    code: Some(NumberOrString::String(d.code.as_str().to_string())),
                    source: Some("rust-analyzer".to_string()),
                    message: d.message,
                    ..Default::default()
                })
            })
            .collect()
    }

    /// Type/parameter inlay hints for the overlaid generated file, anchored in generated-file coordinates. The backend keeps only those whose line maps back to `[logic]`.
    pub fn inlay_hints(&mut self, gen_path: &Path, generated: String) -> Vec<InlayHintRaw> {
        let Some(file_id) = self.file_id(gen_path) else {
            return Vec::new();
        };
        self.overlay(file_id, generated);
        let analysis = self.host.analysis();
        let Ok(hints) = analysis.inlay_hints(&inlay_hints_config(), file_id, None) else {
            return Vec::new();
        };
        let Ok(line_index) = analysis.file_line_index(file_id) else {
            return Vec::new();
        };
        hints
            .into_iter()
            .filter_map(|hint| {
                let anchor = match hint.position {
                    InlayHintPosition::After => hint.range.end(),
                    InlayHintPosition::Before => hint.range.start(),
                };
                let pos = lsp_position(&line_index, anchor);
                let label = hint.label.to_string();
                if label.is_empty() {
                    return None;
                }
                Some(InlayHintRaw {
                    line: pos.line,
                    col: pos.character,
                    pad_left: hint.pad_left,
                    pad_right: hint.pad_right,
                    kind: match hint.kind {
                        InlayKind::Type => Some(InlayHintKind::TYPE),
                        InlayKind::Parameter => Some(InlayHintKind::PARAMETER),
                        _ => None,
                    },
                    label,
                })
            })
            .collect()
    }
}
