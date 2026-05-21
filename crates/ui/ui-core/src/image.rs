use std::rc::Rc;

use layout_core::{LayoutError, LayoutStyle, NodeId};
use platform_core::Event;
use renderer_core::{DrawCommand, ImageData, ImageFilter, Rect};
use ui_tree::{Component, EventResult, View};

use crate::layout_item::LayoutItem;
use crate::layout_leaf::LayoutLeaf;

pub struct Image {
    data: Box<dyn Fn() -> Rc<ImageData>>,
    leaf: LayoutLeaf,
    filter: Box<dyn Fn() -> ImageFilter>,
}

impl Image {
    pub fn new(
        data_fn: impl Fn() -> Rc<ImageData> + 'static,
        style: LayoutStyle,
        filter_fn: impl Fn() -> ImageFilter + 'static,
    ) -> Result<Self, LayoutError> {
        let leaf = LayoutLeaf::register(style)?;
        Ok(Self {
            data: Box::new(data_fn),
            leaf,
            filter: Box::new(filter_fn),
        })
    }
}

impl Component for Image {
    fn view(&self) -> View {
        let r = self.leaf.rect.get();
        View::Translate {
            tx: r.x,
            ty: r.y,
            children: vec![View::Primitive(DrawCommand::Image {
                data: (self.data)(),
                rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: r.width,
                    height: r.height,
                },
                filter: (self.filter)(),
            })],
        }
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }
}

impl LayoutItem for Image {
    fn layout_node(&self) -> NodeId {
        self.leaf.node
    }
}
