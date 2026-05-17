#[derive(Debug, thiserror::Error)]
pub enum LayoutError {
    #[error("node not found in layout tree")]
    InvalidNode,
}
