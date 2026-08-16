//! Shared diagnostic types for the `.rsx` toolchain. `telar-analyzer` is the only consumer today.
//!
//! `.rsx` is whitespace-sensitive and parsed line-by-line, so diagnostics are line-based: producers
//! emit the neutral [`Diagnostic`], and consumers convert it to [`lsp_types::Diagnostic`].

mod diagnostic;
mod lsp;
mod semantic;

pub use diagnostic::{Diagnostic, Severity, Span};
pub use semantic::{CatalogView, ThemeView, semantic_diagnostics};

#[cfg(test)]
mod tests;
