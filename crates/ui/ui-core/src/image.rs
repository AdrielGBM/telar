use std::sync::Arc;

use geometry_core::{ObjectFit, Rect};
use layout_core::{LayoutError, LayoutStyle};
use platform_core::Event;
use renderer_core::{BorderRadius, DrawCommand, ImageData, ImageFilter};
use ui_tree::{Component, EventResult, NodeVec, RenderNode};

use crate::impl_leaf_widget;
use crate::layout_leaf::LayoutLeaf;

pub struct Image {
    data: Box<dyn Fn() -> Arc<ImageData>>,
    leaf: LayoutLeaf,
    filter: Box<dyn Fn() -> ImageFilter>,
    fit: Box<dyn Fn() -> ObjectFit>,
}

impl Image {
    pub fn new(
        ctx: &mut crate::context::WidgetCtx,
        layout_style: LayoutStyle,
        data_fn: impl Fn() -> Arc<ImageData> + 'static,
        filter_fn: impl Fn() -> ImageFilter + 'static,
        fit_fn: impl Fn() -> ObjectFit + 'static,
    ) -> Result<Self, LayoutError> {
        let leaf = LayoutLeaf::register(ctx, layout_style)?;
        Ok(Self {
            data: Box::new(data_fn),
            leaf,
            filter: Box::new(filter_fn),
            fit: Box::new(fit_fn),
        })
    }
}

impl Component for Image {
    fn view(&self) -> RenderNode {
        let r = self.leaf.rect.get();
        let r_local = Rect {
            x: 0.0,
            y: 0.0,
            width: r.width,
            height: r.height,
        };
        let data = (self.data)();
        let (content, clip) = geometry_core::fit_rect(
            (data.width as f32, data.height as f32),
            r_local,
            (self.fit)(),
        );
        let image = RenderNode::Primitive(DrawCommand::Image {
            data,
            rect: content,
            filter: (self.filter)(),
        });
        // Cover overflows the box; clip it to the local box. The renderer maps clip rects through the active matrix, so a local (0,0,w,h) clip composes with this widget's layout transform and any scroll.
        let node = if clip {
            RenderNode::Clip {
                rect: r_local,
                radius: BorderRadius::zero(),
                children: NodeVec::collect([image]),
            }
        } else {
            image
        };
        self.leaf.at_layout_position(node)
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }

    fn debug_name(&self) -> &'static str {
        "Image"
    }
}

impl_leaf_widget!(Image);
