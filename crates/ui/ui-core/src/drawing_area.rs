use crate::impl_leaf_widget;
use crate::layout_leaf::LayoutLeaf;
use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle};
use platform_core::Event;
use ui_tree::{Component, EventResult, RenderNode};

pub struct Canvas {
    leaf: LayoutLeaf,
    draw_fn: Box<dyn Fn(Rect) -> RenderNode>,
}

impl Canvas {
    pub fn new(
        ctx: &mut crate::context::WidgetCtx,
        layout: LayoutStyle,
        draw_fn: impl Fn(Rect) -> RenderNode + 'static,
    ) -> Result<Self, LayoutError> {
        let leaf = LayoutLeaf::register(ctx, layout)?;
        Ok(Self {
            leaf,
            draw_fn: Box::new(draw_fn),
        })
    }
}

impl Canvas {
    pub fn with_intrinsic_height(
        ctx: &mut crate::context::WidgetCtx,
        height: f32,
        draw_fn: impl Fn(geometry_core::Rect) -> ui_tree::RenderNode + 'static,
    ) -> Result<Self, layout_core::LayoutError> {
        Self::new(ctx, layout_core::LayoutStyle::new().height(height), draw_fn)
    }
}

impl_leaf_widget!(Canvas);

impl Component for Canvas {
    fn view(&self) -> RenderNode {
        let r = self.leaf.rect.get();
        let inner = (self.draw_fn)(r);
        self.leaf.at_layout_position(inner)
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }
}
