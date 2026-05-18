// RSX bundler: drives the parse → transpile → compile pipeline for `.rsx` projects.

/// Configuration for a bundler run (placeholder — not yet implemented).
pub struct BundleConfig {
    pub entry: std::path::PathBuf,
    pub out_dir: std::path::PathBuf,
}

/// Run the full RSX build pipeline for the given config.
// TODO: orchestrate rsx-parser → rsx-transpiler → rustc invocation
pub fn bundle(_config: BundleConfig) -> Result<(), BundleError> {
    Err(BundleError::NotImplemented)
}

#[derive(Debug)]
pub enum BundleError {
    NotImplemented,
}

impl std::fmt::Display for BundleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotImplemented => write!(f, "rsx-bundler is not yet implemented"),
        }
    }
}

impl std::error::Error for BundleError {}
