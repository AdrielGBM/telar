//! What the transpiler reports, and the `.rsx` line it points at.

use telar_parser::ParseError;

#[derive(Debug, thiserror::Error)]
/// What the transpiler refused, and where.
pub enum TranspileError {
    #[error("rsx parse error: {0}")]
    Parse(#[from] ParseError),
    #[error("rsx codegen error: {0}")]
    Codegen(String),
}
