//! Shared diagnostic types for the `.rsx` toolchain (parser, transpiler, analyzer, CLI).
//!
//! `.rsx` is whitespace-sensitive and parsed line-by-line, so diagnostics are line-based: producers
//! emit the neutral [`Diagnostic`], and consumers either [`render`](Diagnostic::render) it to a
//! terminal or, behind the `lsp` feature, convert it to [`lsp_types::Diagnostic`].

mod diagnostic;
mod semantic;

pub use diagnostic::{Diagnostic, Severity, Span};
pub use semantic::{ThemeView, semantic_diagnostics};

#[cfg(feature = "lsp")]
mod lsp;

#[cfg(test)]
mod tests;
