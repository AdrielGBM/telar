#[derive(Debug, thiserror::Error)]
pub enum LayoutError {
    #[error("layout error: {0}")]
    Engine(String),
}

impl From<taffy::TaffyError> for LayoutError {
    fn from(e: taffy::TaffyError) -> Self {
        LayoutError::Engine(e.to_string())
    }
}
