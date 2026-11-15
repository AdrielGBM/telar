/// Whether a document recolours what a box draws when its reader asks for forced colours.
///
/// A colour swatch, the area a colour is picked on and a picture are content: forcing their colours would make them say nothing, where a frame or a label has to follow the reader's palette. Every other target ignores it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum ForcedColors {
    /// The box is interface and takes the reader's palette.
    #[default]
    Adjust,
    /// The box and everything under it is colour content and keeps the colours it was drawn in.
    Keep,
}
