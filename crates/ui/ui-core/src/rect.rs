use geometry_core::Rect as Bounds;
use layout_core::{LayoutError, LayoutStyle, NodeId};
use platform_core::Event;
use renderer_core::{DrawCommand, RectPayload, RectStyle};
use ui_tree::{Component, EventResult, View};

use crate::layout_item::LayoutItem;
use crate::layout_leaf::LayoutLeaf;

pub struct Rect {
    leaf: LayoutLeaf,
    style: Box<dyn Fn() -> RectStyle>,
}

impl Rect {
    pub fn new(
        ctx: &mut crate::context::WidgetCtx,
        layout_style: LayoutStyle,
        style: impl Fn() -> RectStyle + 'static,
    ) -> Result<Self, LayoutError> {
        let leaf = LayoutLeaf::register(ctx, layout_style)?;
        Ok(Self {
            leaf,
            style: Box::new(style),
        })
    }
}

impl Component for Rect {
    fn view(&self) -> View {
        let r = self.leaf.rect.get();
        let style = (self.style)();
        self.leaf
            .positioned_view(View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
                rect: Bounds {
                    x: 0.0,
                    y: 0.0,
                    width: r.width,
                    height: r.height,
                },
                style,
            }))))
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }
}

impl LayoutItem for Rect {
    fn layout_node(&self) -> NodeId {
        self.leaf.node
    }
}
