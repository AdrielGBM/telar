use rsx_parser::ParseError;

#[derive(Debug)]
pub enum TranspileError {
    Parse(ParseError),
    Codegen(String),
}

impl std::fmt::Display for TranspileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "rsx parse error: {e}"),
            Self::Codegen(msg) => write!(f, "rsx codegen error: {msg}"),
        }
    }
}

impl std::error::Error for TranspileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Parse(e) => Some(e),
            Self::Codegen(_) => None,
        }
    }
}

impl From<ParseError> for TranspileError {
    fn from(e: ParseError) -> Self {
        Self::Parse(e)
    }
}
