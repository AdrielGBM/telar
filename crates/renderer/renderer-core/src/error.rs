//! What a renderer reports when it cannot be built or cannot draw.

#[derive(Debug, thiserror::Error)]
/// A renderer could not be built, or could not draw a frame.
pub enum RendererError {
    #[error("surface creation failed: {0}")]
    Surface(String),
    #[error("surface resize failed: {0}")]
    Resize(String),
    #[error("frame present failed: {0}")]
    Present(String),
    #[error("backend error: {0}")]
    Backend(String),
}
