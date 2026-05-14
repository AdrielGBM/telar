#[derive(Debug, thiserror::Error)]
pub enum RendererError {
    /// Both renderers: surface creation failed.
    #[error("surface creation failed: {0}")]
    Surface(String),
    /// Both renderers: surface resize failed.
    #[error("surface resize failed: {0}")]
    Resize(String),
    /// Both renderers: presenting the rendered frame failed.
    #[error("frame present failed: {0}")]
    Present(String),
    /// Backend-specific error: provides error details from either hardware or software renderer.
    #[error("backend error: {0}")]
    Backend(String),
}
