use std::cell::RefCell;
use std::rc::Rc;

use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle};
use platform_core::Event;
use renderer_core::{DrawCommand, TextPayload, TextStyle};
use ui_tree::{Component, EventResult, RenderNode};

use crate::layout_item::HasLayoutLeaf;
use crate::layout_leaf::LayoutLeaf;

pub struct Text {
    content_fn: Box<dyn Fn() -> String>,
    cached_content: RefCell<(String, Rc<str>)>,
    style: Box<dyn Fn() -> TextStyle>,
    leaf: LayoutLeaf,
}

impl Text {
    pub fn new(
        ctx: &mut crate::context::WidgetCtx,
        content_fn: impl Fn() -> String + 'static,
        style: LayoutStyle,
        style_fn: impl Fn() -> TextStyle + 'static,
    ) -> Result<Self, LayoutError> {
        let leaf = LayoutLeaf::register(ctx, style)?;
        Ok(Self {
            content_fn: Box::new(content_fn),
            cached_content: RefCell::new((String::new(), Rc::from(""))),
            style: Box::new(style_fn),
            leaf,
        })
    }
}

impl Component for Text {
    fn view(&self) -> RenderNode {
        let r = self.leaf.rect.get();
        let text: Rc<str> = {
            let new_str = (self.content_fn)();
            let mut cache = self.cached_content.borrow_mut();
            if cache.0 != new_str {
                let rc = Rc::from(new_str.as_str());
                *cache = (new_str, Rc::clone(&rc));
                rc
            } else {
                Rc::clone(&cache.1)
            }
        };
        self.leaf
            .positioned_view(RenderNode::Primitive(DrawCommand::Text(Box::new(
                TextPayload {
                    text,
                    rect: Rect {
                        x: 0.0,
                        y: 0.0,
                        width: r.width,
                        height: r.height,
                    },
                    style: (self.style)(),
                },
            ))))
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }
}

impl HasLayoutLeaf for Text {
    fn layout_leaf(&self) -> &LayoutLeaf {
        &self.leaf
    }
}
