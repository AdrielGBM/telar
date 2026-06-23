#[derive(Debug, thiserror::Error)]
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
