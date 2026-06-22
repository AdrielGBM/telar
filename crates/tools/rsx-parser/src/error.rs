#[derive(Debug, thiserror::Error)]
#[error("rsx parse error at line {line}: {message}")]
pub struct ParseError {
    pub message: String,
    pub line: usize,
}
