//! What a backend reports when it cannot open a window or drive its loop.

#[derive(Debug, thiserror::Error)]
#[error("platform error: {0}")]
/// A backend could not open a window or drive its loop.
pub struct PlatformError(pub String);
