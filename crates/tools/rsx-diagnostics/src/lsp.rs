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
        // parser lines are 1-based; LSP lines are 0-based.
        let line = diag.span.line.saturating_sub(1) as u32;
        let (start_char, end_char) = match &diag.span.columns {
            Some(cols) => (cols.start as u32, cols.end as u32),
            None => (0, u32::MAX),
        };
        LspDiagnostic {
            range: Range {
                start: Position {
                    line,
                    character: start_char,
                },
                end: Position {
                    line,
                    character: end_char,
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
