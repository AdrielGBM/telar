//! Hand-written parser for `.rsx` source files (Proposal 4 syntax).
//!
//! A `.rsx` file has three sections:
//! - a leading **logic** zone of verbatim Rust (a `pub struct Props` here declares the component's props),
//! - a `[style]` section of style classes,
//! - a `[view]` section describing an indentation-based node tree.
//!
//! [`parse`] turns the source into an [`RsxDocument`] AST consumed by the transpiler.
//!
//! # An attribute's value
//!
//! Two rules, and no exceptions to them:
//!
//! - **`key:value`** is one token. Parentheses may nest inside it, so `fill:chip(a, b)` and `fill:linear(horizontal, a, b)` read whole; a space at depth 0 ends the value.
//! - **`key(…)`** is anything that needs a space: `cols(240 1fr)`, `stroke_width(0 0 1 0)`, `transition(fill 250ms ease-out)`, `drag_button(secondary auxiliary)`.
//!
//! The parser records *which of the five forms* it read — bare, quoted, `t"…"` for a lookup, a closure, a parenthesised spec — in [`Value`], and does not learn what any of them mean. Meaning belongs to the key schema in `telar-transpiler`'s `registry`, which also holds the closed keyword sets: a value outside one is a build error on the attribute's own line rather than a property silently dropped.
//!
//! Values used to arrive here as bare strings and have their form re-derived downstream — by the transpiler, the analyzer and the formatter separately, each with its own `starts_with('#')` and `contains('(')`. Four unit dialects, three behaviours for a bad value and one run-to-end-of-line parser exception grew in the gap between those re-derivations.
//!
//! [`format::format_document`] is the inverse: it re-serializes that AST into canonical source. It lives beside the parser rather than with the language server so anything holding a `.rsx` file can reach it — the editor through the LSP, `cargo telar fmt` from a terminal — and both give the same answer by construction.

#![warn(rustdoc::broken_intra_doc_links)]

pub mod format;

mod ast;
mod color;
mod error;
mod lexer;
mod parser;

pub use ast::*;
pub use color::parse_hex;
pub use error::ParseError;
pub use lexer::{Section, find_section_at, header_section, is_preview_header};

/// Parses `.rsx` source text into an [`RsxDocument`].
pub fn parse(source: &str) -> Result<RsxDocument, ParseError> {
    parser::Parser::new(source).parse()
}

#[cfg(test)]
#[path = "lib_test.rs"]
mod tests;
