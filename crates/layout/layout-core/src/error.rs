#[derive(Debug, thiserror::Error)]
pub enum LayoutError {
    #[error("taffy layout error: {0}")]
    Taffy(#[from] taffy::TaffyError),
}
