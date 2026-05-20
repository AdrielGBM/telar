use std::rc::Rc;

use platform_core::Event;
use renderer_core::{DrawCommand, Rect, TextStyle};
use ui_tree::{Component, EventResult, View};

pub struct Text {
    text: Box<dyn Fn() -> Rc<str>>,
    rect: Box<dyn Fn() -> Rect>,
    style: Box<dyn Fn() -> TextStyle>,
}

impl Text {
    pub fn new(
        text: impl Fn() -> String + 'static,
        rect: impl Fn() -> Rect + 'static,
        style: impl Fn() -> TextStyle + 'static,
    ) -> Self {
        Self {
            text: Box::new(move || Rc::from(text())),
            rect: Box::new(rect),
            style: Box::new(style),
        }
    }
}

impl Component for Text {
    fn view(&self) -> View {
        View::Primitive(DrawCommand::Text {
            text: (self.text)(),
            rect: (self.rect)(),
            style: (self.style)(),
        })
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }
}
