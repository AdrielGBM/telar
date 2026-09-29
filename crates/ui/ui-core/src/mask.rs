//! [`Mask`]: one subtree seen through another — a word as a window onto the field behind it.

use layout_core::{JustifyContent, LayoutError, LayoutStyle, NodeId};
use platform_core::Event;
use ui_tree::{Component, EventResult, RenderNode};

use crate::context::new_container;
use crate::layout_item::{LayoutItem, TrackedChildren, make_child};
use crate::pointer::dispatch_container_event;

/// Shows `content` only as far as `source` covers it: by the source's alpha where the two overlap, and not at all where it drew nothing. The source is never shown itself; it is the shape the content is seen through.
///
/// The box is laid out by its own style with the content in its flow, and the source laid over the whole of it, centred down its height, so a word set to fill the width is a window the height of its line. Only the content answers the pointer.
///
/// Each target shows it its own way, from the same pair of layers (see `LayerMask`): the GPU and the software rasterizer composite the content through the source's alpha, a document draws the pair as an SVG `<mask>`, and a terminal, which has no alpha to show anything through, draws the content whole and the source not at all. In a document the box is a drawing, so what is inside it is a picture rather than elements of the page: name it with `label:` for a reader.
pub struct Mask {
    node: NodeId,
    source: TrackedChildren,
    content: TrackedChildren,
}

impl Mask {
    pub fn new(
        layout_style: LayoutStyle,
        source: Box<dyn LayoutItem>,
        content: Box<dyn LayoutItem>,
    ) -> Result<Self, LayoutError> {
        let over = new_container(
            LayoutStyle::new()
                .absolute_fill()
                .flex_column()
                .justify_content(JustifyContent::CENTER),
            &[source.layout_node()],
        )?;
        let node = new_container(layout_style, &[content.layout_node(), over])?;
        Ok(Self {
            node,
            source: vec![make_child(source)],
            content: vec![make_child(content)],
        })
    }
}

impl LayoutItem for Mask {
    fn layout_node(&self) -> NodeId {
        self.node
    }
}

impl Component for Mask {
    fn view(&self) -> RenderNode {
        let masked = RenderNode::masked(
            RenderNode::group(self.source.iter().map(|c| c.segment.boundary())),
            RenderNode::group(self.content.iter().map(|c| c.segment.boundary())),
        );
        let element = crate::element::for_target(self.node, || {
            crate::element::with_semantics(self.node, renderer_core::Semantics::drawing())
        });
        RenderNode::element(element, [masked])
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        dispatch_container_event(&mut self.content, event)
    }

    fn debug_name(&self) -> &'static str {
        "Mask"
    }
}

#[cfg(test)]
#[path = "mask_test.rs"]
mod tests;
