use crate::layout_item::HasLayoutLeaf;
use crate::layout_leaf::LayoutLeaf;
use geometry_core::Size;
use layout_core::{LayoutError, LayoutStyle};
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

impl HasLayoutLeaf for DrawingArea {
    fn layout_leaf(&self) -> &LayoutLeaf {
        &self.leaf
    }
}

impl Component for DrawingArea {
    fn view(&self) -> RenderNode {
        let r = self.leaf.rect.get();
        let inner = (self.draw_fn)(Size {
            width: r.width,
            height: r.height,
        });
        self.leaf.at_layout_position(inner)
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }
}
