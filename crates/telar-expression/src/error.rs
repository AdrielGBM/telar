//! [`Error`]: everything that can go wrong with an expression, located, and [`render`] to show one.

use std::fmt;

use crate::{ErrorCode, Span};

/// Which stage refused the expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    /// The text is not an expression: a stray character, an unclosed bracket, a missing operand.
    Syntax,
    /// The expression reads, but does not fit together: a reference or function nobody declared, or a value of the wrong type.
    Type,
    /// A well-typed expression that has no answer for the values it read this time: a division by zero, an index past the end.
    Evaluation,
}

/// A problem with an expression, and the bytes of the source it is about.
///
/// The evaluator returns these rather than panicking, so a host can show one beside the expression that caused it and go on drawing everything else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub kind: ErrorKind,
    pub span: Span,
    pub code: ErrorCode,
}

impl Error {
    pub fn new(kind: ErrorKind, span: Span, code: ErrorCode) -> Self {
        Self { kind, span, code }
    }

    pub(crate) fn syntax(span: Span, code: ErrorCode) -> Self {
        Self::new(ErrorKind::Syntax, span, code)
    }

    pub(crate) fn mismatch(span: Span, code: ErrorCode) -> Self {
        Self::new(ErrorKind::Type, span, code)
    }

    pub(crate) fn evaluation(span: Span, code: ErrorCode) -> Self {
        Self::new(ErrorKind::Evaluation, span, code)
    }

    /// The English wording of [`Error::code`], without the source line.
    pub fn message(&self) -> String {
        self.code.to_string()
    }

    /// [`render`] for this error.
    pub fn render(&self, source: &str) -> String {
        render(source, self)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.code.fmt(f)
    }
}

impl std::error::Error for Error {}

/// The error as a terminal or a report shows it: what went wrong, then the source line with a caret under the span.
///
/// ```text
/// type error: unknown name `firefox`
///   |
/// 1 | 2 + 3 firefox
///   |       ^^^^^^^
/// ```
///
/// A span past the end of `source`, or inside a character, is pulled back to the nearest boundary, so a stale error rendered against edited text still draws.
pub fn render(source: &str, error: &Error) -> String {
    let start = floor_boundary(source, error.span.start);
    let end = floor_boundary(source, error.span.end.max(start));
    let line_start = source[..start].rfind('\n').map_or(0, |at| at + 1);
    let line_end = source[start..]
        .find('\n')
        .map_or(source.len(), |at| start + at);
    let line_number = source[..start].matches('\n').count() + 1;
    let column = source[line_start..start].chars().count();
    let width = source[start..end.min(line_end)].chars().count().max(1);
    let line: String = source[line_start..line_end]
        .chars()
        .map(|c| if c == '\t' { ' ' } else { c })
        .collect();
    let gutter = line_number.to_string();
    let pad = " ".repeat(gutter.len());
    let label = match error.kind {
        ErrorKind::Syntax => "syntax error",
        ErrorKind::Type => "type error",
        ErrorKind::Evaluation => "evaluation error",
    };
    format!(
        "{label}: {message}\n{pad} |\n{gutter} | {line}\n{pad} | {indent}{carets}",
        message = error.code,
        indent = " ".repeat(column),
        carets = "^".repeat(width),
    )
}

fn floor_boundary(source: &str, at: usize) -> usize {
    let mut at = at.min(source.len());
    while !source.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// Why an expression did not compile: one error or more, in the order of the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Errors {
    errors: Vec<Error>,
}

impl Errors {
    pub(crate) fn from_vec(mut errors: Vec<Error>) -> Option<Self> {
        if errors.is_empty() {
            return None;
        }
        errors.sort_by_key(|error| (error.span.start, error.span.end));
        Some(Self { errors })
    }

    /// The error earliest in the source.
    pub fn first(&self) -> &Error {
        &self.errors[0]
    }

    pub fn into_first(self) -> Error {
        self.errors
            .into_iter()
            .next()
            .expect("Errors is never empty")
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Error> {
        self.errors.iter()
    }
}

impl From<Error> for Errors {
    fn from(error: Error) -> Self {
        Self {
            errors: vec![error],
        }
    }
}

impl IntoIterator for Errors {
    type Item = Error;
    type IntoIter = std::vec::IntoIter<Error>;

    fn into_iter(self) -> Self::IntoIter {
        self.errors.into_iter()
    }
}

impl fmt::Display for Errors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, error) in self.iter().enumerate() {
            if index > 0 {
                f.write_str("; ")?;
            }
            error.fmt(f)?;
        }
        Ok(())
    }
}

impl std::error::Error for Errors {}

#[cfg(test)]
#[path = "error_test.rs"]
mod tests;
