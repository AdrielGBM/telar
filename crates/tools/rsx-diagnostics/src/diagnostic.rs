use std::fmt::Write;
use std::ops::Range;

use rsx_parser::ParseError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
            Severity::Hint => "hint",
        }
    }
}

/// Source location of a [`Diagnostic`]. The line is always known (1-based, matching the parser);
/// `columns` carry char-offset precision only when a producer has it, otherwise the whole line is spanned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub line: usize,
    pub columns: Option<Range<usize>>,
}

impl Span {
    pub fn line(line: usize) -> Self {
        Self {
            line,
            columns: None,
        }
    }

    pub fn at(line: usize, columns: Range<usize>) -> Self {
        Self {
            line,
            columns: Some(columns),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub span: Span,
    pub code: Option<String>,
}

impl Diagnostic {
    pub fn new(severity: Severity, message: impl Into<String>, span: Span) -> Self {
        Self {
            severity,
            message: message.into(),
            span,
            code: None,
        }
    }

    pub fn error(message: impl Into<String>, span: Span) -> Self {
        Self::new(Severity::Error, message, span)
    }

    pub fn warning(message: impl Into<String>, span: Span) -> Self {
        Self::new(Severity::Warning, message, span)
    }

    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    /// Renders the diagnostic against its `source` as a rustc-style, color-free block for terminals.
    pub fn render(&self, source: &str) -> String {
        let mut out = String::new();
        match &self.code {
            Some(code) => {
                let _ = write!(out, "{}[{code}]", self.severity.label());
            }
            None => {
                let _ = write!(out, "{}", self.severity.label());
            }
        }
        let _ = writeln!(out, ": {}", self.message);

        let line_no = self.span.line;
        let text = source.lines().nth(line_no.saturating_sub(1)).unwrap_or("");
        let gutter = line_no.to_string();
        let pad = " ".repeat(gutter.len());

        let _ = writeln!(out, "{pad}--> line {line_no}");
        let _ = writeln!(out, "{pad} |");
        let _ = writeln!(out, "{gutter} | {text}");

        // Underline the offending columns, or the whole non-blank span of the line when unknown.
        let (lead, len) = match &self.span.columns {
            Some(cols) => (cols.start, cols.len().max(1)),
            None => {
                let lead = text.len() - text.trim_start().len();
                (lead, text.trim().chars().count().max(1))
            }
        };
        let _ = writeln!(out, "{pad} | {}{}", " ".repeat(lead), "^".repeat(len));
        out
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
