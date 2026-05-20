use std::rc::Rc;

use platform_core::Event;
use renderer_core::{DrawCommand, Rect, TextStyle};
use ui_tree::{Component, EventResult, View};

pub struct Text {
    text: Box<dyn Fn() -> Rc<str>>,
    rect: Box<dyn Fn() -> Rect>,
    style: TextStyle,
}

impl Text {
    pub fn new(text: impl Into<String>, rect: Rect, style: TextStyle) -> Self {
        let s: Rc<str> = Rc::from(text.into());
        Self {
            text: Box::new(move || Rc::clone(&s)),
            rect: Box::new(move || rect),
            style,
        }
    }

    pub fn from_fn(
        text: impl Fn() -> String + 'static,
        rect: impl Fn() -> Rect + 'static,
        style: TextStyle,
    ) -> Self {
        Self {
            text: Box::new(move || Rc::from(text())),
            rect: Box::new(rect),
            style,
        }
    }
}

impl Component for Text {
    fn view(&self) -> View {
        View::Primitive(DrawCommand::Text {
            text: (self.text)(),
            rect: (self.rect)(),
            style: self.style,
        })
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }
}
