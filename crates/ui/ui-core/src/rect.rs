use platform_core::Event;
use renderer_core::{DrawCommand, Rect as RectBounds, RectStyle};
use ui_tree::{Component, EventResult, View};

pub struct Rect {
    rect: Box<dyn Fn() -> RectBounds>,
    style: Box<dyn Fn() -> RectStyle>,
}

impl Rect {
    pub fn new(rect: RectBounds, style: RectStyle) -> Self {
        Self {
            rect: Box::new(move || rect),
            style: Box::new(move || style),
        }
    }

    pub fn from_fn(
        rect: impl Fn() -> RectBounds + 'static,
        style: impl Fn() -> RectStyle + 'static,
    ) -> Self {
        Self {
            rect: Box::new(rect),
            style: Box::new(style),
        }
    }
}

impl Component for Rect {
    fn view(&self) -> View {
        let rect = (self.rect)();
        let style = (self.style)();
        View::Primitive(DrawCommand::Rect { rect, style })
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }
}
