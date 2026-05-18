// RSX transpiler: converts an RSX AST into Rust source code.

/// A transpilation result containing generated Rust source (placeholder — not yet implemented).
pub struct TranspiledSource {
    pub rust_code: String,
}

/// Transpile an RSX document AST into Rust source.
// TODO: implement code generation from the parsed AST
pub fn transpile(_input: &str) -> Result<TranspiledSource, TranspileError> {
    Err(TranspileError::NotImplemented)
}

#[derive(Debug)]
pub enum TranspileError {
    NotImplemented,
}

impl std::fmt::Display for TranspileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotImplemented => write!(f, "rsx-transpiler is not yet implemented"),
        }
    }
}

impl std::error::Error for TranspileError {}
