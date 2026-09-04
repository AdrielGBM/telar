//! What the parser reports, and where in the `.rsx` it happened.

#[derive(Debug, thiserror::Error)]
#[error("rsx parse error at line {line}: {message}")]
/// What went wrong, and the `.rsx` line it happened on.
pub struct ParseError {
    pub message: String,
    pub line: usize,
}
