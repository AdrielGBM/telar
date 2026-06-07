use std::rc::Rc;

use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle};
use platform_core::Event;
use renderer_core::{DrawCommand, ImageData, ImageFilter};
use ui_tree::{Component, EventResult, RenderNode};

use crate::impl_leaf_widget;
use crate::layout_leaf::LayoutLeaf;

pub struct Image {
    data: Box<dyn Fn() -> Rc<ImageData>>,
    leaf: LayoutLeaf,
    filter: Box<dyn Fn() -> ImageFilter>,
}

impl Image {
    pub fn new(
        ctx: &mut crate::context::WidgetCtx,
        data_fn: impl Fn() -> Rc<ImageData> + 'static,
        layout: LayoutStyle,
        filter_fn: impl Fn() -> ImageFilter + 'static,
    ) -> Result<Self, LayoutError> {
        let leaf = LayoutLeaf::register(ctx, layout)?;
        Ok(Self {
            data: Box::new(data_fn),
            leaf,
            filter: Box::new(filter_fn),
        })
    }
}

impl Component for Image {
    fn view(&self) -> RenderNode {
        let r = self.leaf.rect.get();
        self.leaf
            .at_layout_position(RenderNode::Primitive(DrawCommand::Image {
                data: (self.data)(),
                rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: r.width,
                    height: r.height,
                },
                filter: (self.filter)(),
            }))
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }
}

impl_leaf_widget!(Image);
