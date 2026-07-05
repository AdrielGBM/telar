use crate::impl_leaf_widget;
use crate::layout_leaf::LayoutLeaf;
use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle};
use platform_core::Event;
use ui_tree::{Component, EventResult, RenderNode};

pub struct Canvas {
    leaf: LayoutLeaf,
    draw: Box<dyn Fn(Rect) -> RenderNode>,
}

impl Canvas {
    pub fn new(
        ctx: &mut crate::context::WidgetCtx,
        layout_style: LayoutStyle,
        draw_fn: impl Fn(Rect) -> RenderNode + 'static,
    ) -> Result<Self, LayoutError> {
        let leaf = LayoutLeaf::register(ctx, layout_style)?;
        Ok(Self {
            leaf,
            draw: Box::new(draw_fn),
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
        // A Canvas closure draws at fixed coordinates that ignore the layout rect, so a collapsed rect
        // (e.g. a section hidden via `display:none`) would still paint over other content. Draw nothing.
        if r.width <= 0.0 || r.height <= 0.0 {
            return RenderNode::Empty;
        }
        // The closure draws in local space (at_layout_position translates the output), so it gets a zero-origin rect — passing the absolute layout rect would double-offset anything derived from rect.x/y.
        let local = Rect {
            x: 0.0,
            y: 0.0,
            width: r.width,
            height: r.height,
        };
        let inner = (self.draw)(local);
        self.leaf.at_layout_position(inner)
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }

    fn debug_name(&self) -> &'static str {
        "Canvas"
    }
}
