//! The room around and inside a box, as layout resolved it.

use crate::{Insets, Size};

/// A box's padding, margin, border and gap in logical pixels, every one already resolved against the box's own size, the surface and the writing direction.
///
/// Edges are physical, whatever side a logical edge came from. A gap that was never set is zero.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BoxModel {
    pub padding: Insets,
    pub margin: Insets,
    pub border: Insets,
    /// `width` is the gap between columns, `height` the gap between rows.
    pub gap: Size,
}
