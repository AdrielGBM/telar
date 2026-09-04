//! What can go wrong laying a tree out.

#[derive(Debug, thiserror::Error)]
/// What can go wrong creating, mutating or computing a layout tree.
pub enum LayoutError {
    #[error("layout error: {0}")]
    Engine(String),
}

impl From<taffy::TaffyError> for LayoutError {
    fn from(e: taffy::TaffyError) -> Self {
        LayoutError::Engine(e.to_string())
    }
}
