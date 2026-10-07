//! [`LayoutLeaf`]: the shared base for a widget that measures its own content and draws at its laid-out rect.

use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle, MeasureInput, NodeId};
use reactive_core::RwSignal;
use ui_tree::RenderNode;

use crate::context;

/// A layout node and the rect layout last gave it: what a widget that draws its own content stands on.
///
/// Public so a widget outside the kernel can be a leaf of its own rather than a composition of the kernel's, and be placed the way every kernel leaf is: through [`at_layout_position`](Self::at_layout_position), which is where a document backend learns that the box exists.
pub struct LayoutLeaf {
    pub node: NodeId,
    pub rect: RwSignal<Rect>,
}

impl LayoutLeaf {
    pub fn register(layout_style: LayoutStyle) -> Result<Self, LayoutError> {
        let (node, rect) = context::new_leaf(layout_style)?;
        Ok(Self { node, rect })
    }

    /// A leaf sized by `measure`, which layout asks for a `(width, height)` in the space it offers. Layout only asks again once the node is dirty, so a leaf whose measure reads state calls [`mark_dirty`](crate::mark_dirty) on its node when that state changes.
    pub fn measured(
        layout_style: LayoutStyle,
        measure: impl FnMut(MeasureInput) -> (f32, f32) + 'static,
    ) -> Result<Self, LayoutError> {
        let (node, rect) = context::new_measured_leaf(layout_style, Box::new(measure))?;
        Ok(Self { node, rect })
    }

    /// `content`, drawn from the leaf's top-left corner, wherever layout put it.
    pub fn at_layout_position(&self, content: RenderNode) -> RenderNode {
        self.at_layout_position_as(renderer_core::Semantics::group, content)
    }

    /// As [`Self::at_layout_position_as`], for a picture a document can show by its address: there the box is the picture, and `content` is what every other target draws.
    pub(crate) fn picture_at_layout_position(
        &self,
        picture: impl FnOnce() -> Option<renderer_core::Picture>,
        semantics: impl FnOnce() -> renderer_core::Semantics,
        content: RenderNode,
    ) -> RenderNode {
        if ui_tree::element_capture()
            && let Some(picture) = picture()
        {
            let element = crate::element::showing(self.node, semantics(), picture);
            return RenderNode::element(element, [content]);
        }
        self.at_layout_position_as(semantics, content)
    }

    /// As [`Self::at_layout_position`], for a leaf that is more than a box — artwork, a bitmap — and has to say so where the box becomes an element. `semantics` is only called on that target.
    pub(crate) fn at_layout_position_as(
        &self,
        semantics: impl FnOnce() -> renderer_core::Semantics,
        content: RenderNode,
    ) -> RenderNode {
        // Every leaf is placed through here, which makes it the one place a document backend has to be told about them. The translation is dropped there rather than carried: it says where the box goes, which on that target is already what the element says, and emitting both stepped a row of cards diagonally down the page.
        if ui_tree::element_capture() {
            let element = crate::element::with_semantics(self.node, semantics());
            return RenderNode::element(element, [content]);
        }
        let r = self.rect.get();
        RenderNode::element(
            crate::element::identity(self.node),
            [RenderNode::translate(r.x, r.y, [content])],
        )
    }
}

/// Resolves the `auto` sides of a media leaf (img/svg) against an intrinsic size: both auto → the intrinsic size; one auto with the other a px length → derive the auto side from the intrinsic aspect ratio; a percent side is left untouched. `intrinsic` is only evaluated when needed.
pub(crate) fn resolve_intrinsic_size(
    style: LayoutStyle,
    intrinsic: impl FnOnce() -> (f32, f32),
) -> LayoutStyle {
    match (style.is_width_auto(), style.is_height_auto()) {
        (true, true) => {
            let (iw, ih) = intrinsic();
            style.width(iw).height(ih)
        }
        (true, false) => match style.height_px() {
            Some(h) => {
                let (iw, ih) = intrinsic();
                if ih > 0.0 {
                    style.width(h * iw / ih)
                } else {
                    style
                }
            }
            None => style,
        },
        (false, true) => match style.width_px() {
            Some(w) => {
                let (iw, ih) = intrinsic();
                if iw > 0.0 {
                    style.height(w * ih / iw)
                } else {
                    style
                }
            }
            None => style,
        },
        (false, false) => style,
    }
}
