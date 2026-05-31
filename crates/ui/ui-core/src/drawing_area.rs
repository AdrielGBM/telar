use crate::layout_item::LayoutItem;
use crate::layout_leaf::LayoutLeaf;
use geometry_core::Size;
use layout_core::{LayoutError, LayoutStyle, NodeId};
use platform_core::Event;
use ui_tree::{Component, EventResult, RenderNode};

pub struct DrawingArea {
    leaf: LayoutLeaf,
    draw_fn: Box<dyn Fn(Size) -> RenderNode>,
}

impl DrawingArea {
    pub fn new(
        ctx: &mut crate::context::WidgetCtx,
        style: LayoutStyle,
        draw_fn: impl Fn(Size) -> RenderNode + 'static,
    ) -> Result<Self, LayoutError> {
        let leaf = LayoutLeaf::register(ctx, style)?;
        Ok(Self {
            leaf,
            draw_fn: Box::new(draw_fn),
        })
    }
}

impl LayoutItem for DrawingArea {
    fn layout_node(&self) -> NodeId {
        self.leaf.node
    }
}

impl Component for DrawingArea {
    fn view(&self) -> RenderNode {
        let r = self.leaf.rect.get();
        let inner = (self.draw_fn)(Size {
            width: r.width,
            height: r.height,
        });
        self.leaf.positioned_view(inner)
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }
}
