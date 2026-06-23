use geometry_core::Point;
use platform_core::Event;
use renderer_core::{DrawCommand, Stroke};
use ui_tree::{Component, EventResult, RenderNode};

/// `Line` is designed for use inside `Canvas` closures where you control absolute coordinates. It does not implement `LayoutItem` because its `p1`/`p2` points are absolute, not relative to a layout rect. To use `Line` in a layout context, embed it in a `Canvas` widget.
pub struct Line {
    p1: Box<dyn Fn() -> Point>,
    p2: Box<dyn Fn() -> Point>,
    style: Box<dyn Fn() -> Stroke>,
}

impl Line {
    pub fn new(
        p1: impl Fn() -> Point + 'static,
        p2: impl Fn() -> Point + 'static,
        style: impl Fn() -> Stroke + 'static,
    ) -> Self {
        Self {
            p1: Box::new(p1),
            p2: Box::new(p2),
            style: Box::new(style),
        }
    }
}

impl Component for Line {
    fn view(&self) -> RenderNode {
        let p1 = (self.p1)();
        let p2 = (self.p2)();
        let style = (self.style)();
        RenderNode::Primitive(DrawCommand::Line { p1, p2, style })
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }
}
