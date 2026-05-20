use platform_core::Event;
use renderer_core::{DrawCommand, LineStyle, Point};
use ui_tree::{Component, EventResult, View};

pub struct Line {
    p1: Box<dyn Fn() -> Point>,
    p2: Box<dyn Fn() -> Point>,
    style: Box<dyn Fn() -> LineStyle>,
}

impl Line {
    pub fn new(
        p1: impl Fn() -> Point + 'static,
        p2: impl Fn() -> Point + 'static,
        style: impl Fn() -> LineStyle + 'static,
    ) -> Self {
        Self {
            p1: Box::new(p1),
            p2: Box::new(p2),
            style: Box::new(style),
        }
    }
}

impl Component for Line {
    fn view(&self) -> View {
        let p1 = (self.p1)();
        let p2 = (self.p2)();
        let style = (self.style)();
        View::Primitive(DrawCommand::Line { p1, p2, style })
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }
}
