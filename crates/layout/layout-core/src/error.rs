#[derive(Debug, thiserror::Error)]
pub enum LayoutError {
    #[error("node not found in layout tree")]
    InvalidNode,
    #[error("taffy layout error: {0}")]
    Taffy(#[from] taffy::TaffyError),
}
