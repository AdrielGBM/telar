#[derive(Debug, thiserror::Error)]
#[error("platform error: {0}")]
pub struct PlatformError(pub String);
