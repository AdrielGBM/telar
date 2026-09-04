//! What a diagnostic is here: a message, a severity and the `.rsx` span it points at.

use telar_parser::ParseError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// How bad a diagnostic is.
pub enum Severity {
    Error,
    Warning,
}

/// Source location of a [`Diagnostic`]: the 1-based line (matching the parser). The whole line is spanned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub line: usize,
}

impl Span {
    pub fn line(line: usize) -> Self {
        Self { line }
    }
}

#[derive(Debug, Clone)]
/// A message, a severity, and the `.rsx` line it points at.
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub span: Span,
}

impl Diagnostic {
    pub fn new(severity: Severity, message: impl Into<String>, span: Span) -> Self {
        Self {
            severity,
            message: message.into(),
            span,
        }
    }

    pub fn error(message: impl Into<String>, span: Span) -> Self {
        Self::new(Severity::Error, message, span)
    }

    pub fn warning(message: impl Into<String>, span: Span) -> Self {
        Self::new(Severity::Warning, message, span)
    }
}

impl From<&ParseError> for Diagnostic {
    fn from(err: &ParseError) -> Self {
        Diagnostic::error(err.message.clone(), Span::line(err.line))
    }
}

impl From<ParseError> for Diagnostic {
    fn from(err: ParseError) -> Self {
        Diagnostic::from(&err)
    }
}
