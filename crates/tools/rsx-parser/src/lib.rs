// RSX syntax parser: tokenizes and parses `.rsx` source files into an AST.

/// A parsed RSX document (placeholder — not yet implemented).
pub struct RsxDocument;

/// Parse RSX source text into a document.
// TODO: implement tokenizer and recursive-descent parser
pub fn parse(_source: &str) -> Result<RsxDocument, ParseError> {
    Err(ParseError::NotImplemented)
}

#[derive(Debug)]
pub enum ParseError {
    NotImplemented,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotImplemented => write!(f, "rsx-parser is not yet implemented"),
        }
    }
}

impl std::error::Error for ParseError {}
