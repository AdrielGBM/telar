use std::rc::Rc;

use layout_core::{LayoutError, LayoutStyle, NodeId};
use platform_core::Event;
use renderer_core::{DrawCommand, Rect, TextStyle};
use ui_tree::{Component, EventResult, View};

use crate::layout_item::LayoutItem;
use crate::layout_leaf::LayoutLeaf;

pub struct Text {
    content: Box<dyn Fn() -> Rc<str>>,
    style: Box<dyn Fn() -> TextStyle>,
    leaf: LayoutLeaf,
}

impl Text {
    pub fn new(
        content_fn: impl Fn() -> String + 'static,
        style: LayoutStyle,
        style_fn: impl Fn() -> TextStyle + 'static,
    ) -> Result<Self, LayoutError> {
        let leaf = LayoutLeaf::register(style)?;
        Ok(Self {
            content: Box::new(move || Rc::from(content_fn())),
            style: Box::new(style_fn),
            leaf,
        })
    }
}

impl Component for Text {
    fn view(&self) -> View {
        let r = self.leaf.rect.get();
        View::Translate {
            tx: r.x,
            ty: r.y,
            children: vec![View::Primitive(DrawCommand::Text {
                text: (self.content)(),
                rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: r.width,
                    height: r.height,
                },
                style: (self.style)(),
            })],
        }
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }
}

impl LayoutItem for Text {
    fn layout_node(&self) -> NodeId {
        self.leaf.node
    }
}
