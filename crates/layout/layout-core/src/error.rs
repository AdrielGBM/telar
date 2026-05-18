#[derive(Debug, thiserror::Error)]
pub enum LayoutError {
    #[error("layout error: {0}")]
    Layout(String),
}

impl From<taffy::TaffyError> for LayoutError {
    fn from(e: taffy::TaffyError) -> Self {
        LayoutError::Layout(e.to_string())
    }
}
