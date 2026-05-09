#[derive(Debug, thiserror::Error)]
pub enum RendererError {
    /// Hardware renderer only: no suitable GPU adapter found.
    #[error("no suitable GPU adapter found")]
    NoAdapter,
    /// Both renderers: surface creation failed.
    #[error("GPU surface creation failed: {0}")]
    Surface(String),
    /// Hardware renderer only: GPU device request failed.
    #[error("GPU device request failed: {0}")]
    Device(String),
    /// Software renderer only: softbuffer context creation failed.
    #[error("softbuffer context creation failed: {0}")]
    Context(String),
    /// Both renderers: surface resize failed.
    #[error("surface resize failed: {0}")]
    Resize(String),
    /// Both renderers: presenting the rendered frame failed.
    #[error("frame present failed: {0}")]
    Present(String),
}
