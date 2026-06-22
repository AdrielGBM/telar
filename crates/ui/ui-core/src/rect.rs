use geometry_core::Rect as Bounds;
use layout_core::{LayoutError, LayoutStyle};
use platform_core::Event;
use renderer_core::RectStyle;
use ui_tree::{Component, EventResult, RenderNode};

use crate::impl_leaf_widget;
use crate::layout_leaf::LayoutLeaf;

pub struct Rectangle {
    leaf: LayoutLeaf,
    style: Box<dyn Fn() -> RectStyle>,
}

impl Rectangle {
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

impl Component for Rectangle {
    fn view(&self) -> RenderNode {
        let r = self.leaf.rect.get();
        let style = (self.style)();
        self.leaf.at_layout_position(RenderNode::rect(
            Bounds {
                x: 0.0,
                y: 0.0,
                width: r.width,
                height: r.height,
            },
            style,
        ))
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }
}

impl_leaf_widget!(Rectangle);
