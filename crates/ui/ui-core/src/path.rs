use std::rc::Rc;

use platform_core::Event;
use renderer_core::{DrawCommand, PathData, PathPayload, PathStyle};
use ui_tree::{Component, EventResult, RenderNode};

/// `Path` is designed for use inside `Canvas`/`DrawingArea` closures where you control absolute coordinates. It does not implement `LayoutItem` because its path data uses absolute points, not relative to a layout rect. To use `Path` in a layout context, embed it in a `Canvas` widget.
pub struct Path {
    data: Box<dyn Fn() -> Rc<PathData>>,
    style: Box<dyn Fn() -> PathStyle>,
}

impl Path {
    pub fn new(
        data: impl Fn() -> Rc<PathData> + 'static,
        style: impl Fn() -> PathStyle + 'static,
    ) -> Self {
        Self {
            data: Box::new(data),
            style: Box::new(style),
        }
    }

    pub fn static_data(data: Rc<PathData>, style: impl Fn() -> PathStyle + 'static) -> Self {
        Self {
            data: Box::new(move || data.clone()),
            style: Box::new(style),
        }
    }
}

impl Component for Path {
    fn view(&self) -> RenderNode {
        let data = (self.data)();
        let style = (self.style)();
        RenderNode::Primitive(DrawCommand::Path(Box::new(PathPayload { data, style })))
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }
}
