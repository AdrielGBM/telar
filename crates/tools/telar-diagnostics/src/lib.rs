//! Shared diagnostic types and the `.rsx` semantic checks, used by `telar-analyzer` in the editor and `cargo-telar` in `cargo telar check`.
//!
//! `.rsx` is whitespace-sensitive and parsed line-by-line, so diagnostics are line-based: producers emit the neutral [`Diagnostic`], and consumers convert it to [`lsp_types::Diagnostic`].

#![warn(rustdoc::broken_intra_doc_links)]

mod diagnostic;
#[cfg(feature = "lsp")]
mod lsp;
mod semantic;

pub use diagnostic::{Diagnostic, Severity, Span};
pub use semantic::{CatalogView, semantic_diagnostics};

#[cfg(test)]
#[path = "lib.test.rs"]
mod tests;
