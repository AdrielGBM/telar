//! What the transpiler reports, and the `.rsx` line it points at.

use telar_parser::ParseError;

#[derive(Debug, thiserror::Error)]
/// What the transpiler refused, and where.
pub enum TranspileError {
    #[error("rsx parse error: {0}")]
    Parse(#[from] ParseError),
    #[error("rsx codegen error: {0}")]
    Codegen(String),
    /// The file's name has no spelling in the Rust it would be generated as.
    #[error("{0}")]
    Unnamed(#[from] telar_project::NotUtf8Path),
}
