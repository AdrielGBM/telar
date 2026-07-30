//! Shared diagnostic types for the `.rsx` toolchain (parser, transpiler, analyzer, CLI).
//!
//! `.rsx` is whitespace-sensitive and parsed line-by-line, so diagnostics are line-based: producers
//! emit the neutral [`Diagnostic`], and consumers convert it to [`lsp_types::Diagnostic`] behind the
//! `lsp` feature.

mod diagnostic;
mod semantic;

pub use diagnostic::{Diagnostic, Severity, Span};
pub use semantic::{CatalogView, ThemeView, semantic_diagnostics};

#[cfg(feature = "lsp")]
mod lsp;

#[cfg(test)]
mod tests;
