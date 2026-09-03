use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle, NodeId};
use reactive_core::RwSignal;
use ui_tree::RenderNode;

use crate::context;

pub(crate) struct LayoutLeaf {
    pub node: NodeId,
    pub rect: RwSignal<Rect>,
}

impl LayoutLeaf {
    pub fn register(layout_style: LayoutStyle) -> Result<Self, LayoutError> {
        let (node, rect) = context::new_leaf(layout_style)?;
        Ok(Self { node, rect })
    }

    pub(crate) fn at_layout_position(&self, content: RenderNode) -> RenderNode {
        self.at_layout_position_as(renderer_core::Semantics::group, content)
    }

    /// As [`Self::at_layout_position`], for a leaf that is more than a box — artwork, a bitmap — and has to
    /// say so where the box becomes an element. `semantics` is only called on that target.
    pub(crate) fn at_layout_position_as(
        &self,
        semantics: impl FnOnce() -> renderer_core::Semantics,
        content: RenderNode,
    ) -> RenderNode {
        // Every leaf is placed through here, which is what makes this the one place a document backend has
        // to be told about them: a leaf that owns a layout node and no element is a hole in the tree the
        // browser lays out, and its siblings lose the box that was meant to contain them.
        //
        // The translation is *dropped* there rather than carried, and that is the point of the branch: it
        // says where the box goes, which on that target is already what the element says. Emitting both put
        // every leaf at its layout position twice — a row of cards came out stepping diagonally down the page.
        if ui_tree::element_capture() {
            let element = crate::element::with_semantics(self.node, semantics());
            return RenderNode::element(element, [content]);
        }
        let r = self.rect.get();
        RenderNode::translate(r.x, r.y, [content])
    }
}

/// Resolves the `auto` sides of a media leaf (img/svg) against an intrinsic size: both auto → the
/// intrinsic size; one auto with the other a px length → derive the auto side from the intrinsic
/// aspect ratio; a percent side is left untouched. `intrinsic` is only evaluated when needed.
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
