use lsp_types::{Diagnostic as LspDiagnostic, DiagnosticSeverity, NumberOrString, Position, Range};

use crate::{Diagnostic, Severity};

impl From<Severity> for DiagnosticSeverity {
    fn from(severity: Severity) -> Self {
        match severity {
            Severity::Error => DiagnosticSeverity::ERROR,
            Severity::Warning => DiagnosticSeverity::WARNING,
            Severity::Info => DiagnosticSeverity::INFORMATION,
            Severity::Hint => DiagnosticSeverity::HINT,
        }
    }
}

impl From<&Diagnostic> for LspDiagnostic {
    fn from(diag: &Diagnostic) -> Self {
        // parser lines are 1-based; LSP lines are 0-based. Diagnostics span the whole line.
        let line = diag.span.line.saturating_sub(1) as u32;
        LspDiagnostic {
            range: Range {
                start: Position { line, character: 0 },
                end: Position {
                    line,
                    character: u32::MAX,
                },
            },
            severity: Some(diag.severity.into()),
            code: diag.code.clone().map(NumberOrString::String),
            message: diag.message.clone(),
            ..Default::default()
        }
    }
}

impl From<Diagnostic> for LspDiagnostic {
    fn from(diag: Diagnostic) -> Self {
        LspDiagnostic::from(&diag)
    }
}
