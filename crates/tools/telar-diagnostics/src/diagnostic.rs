use telar_parser::ParseError;

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
