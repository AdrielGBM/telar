use std::sync::Arc;

use platform_core::Event;
use renderer_core::{PathData, PathStyle};
use ui_tree::{Component, EventResult, RenderNode};

/// `Path` is designed for use inside `Canvas` closures where you control absolute coordinates. It does not implement `LayoutItem` because its path data uses absolute points, not relative to a layout rect. To use `Path` in a layout context, embed it in a `Canvas` widget.
pub struct Path {
    data: Box<dyn Fn() -> Arc<PathData>>,
    style: Box<dyn Fn() -> PathStyle>,
}

impl Path {
    pub fn new(
        data: impl Fn() -> Arc<PathData> + 'static,
        style: impl Fn() -> PathStyle + 'static,
    ) -> Self {
        Self {
            data: Box::new(data),
            style: Box::new(style),
        }
    }

    pub fn static_data(data: Arc<PathData>, style: impl Fn() -> PathStyle + 'static) -> Self {
        Self::new(move || data.clone(), style)
    }
}

impl Component for Path {
    fn view(&self) -> RenderNode {
        let data = (self.data)();
        let style = (self.style)();
        RenderNode::path(data, style)
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }
}
