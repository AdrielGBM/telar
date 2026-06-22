use rsx_parser::ParseError;

#[derive(Debug, thiserror::Error)]
pub enum TranspileError {
    #[error("rsx parse error: {0}")]
    Parse(#[from] ParseError),
    #[error("rsx codegen error: {0}")]
    Codegen(String),
}
