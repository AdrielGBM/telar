use std::rc::Rc;

use platform_core::Event;
use renderer_core::{DrawCommand, ImageData, ImageFilter, Rect};
use ui_tree::{Component, EventResult, View};

pub struct Image {
    data: Box<dyn Fn() -> Rc<ImageData>>,
    rect: Box<dyn Fn() -> Rect>,
    filter: ImageFilter,
}

impl Image {
    pub fn new(
        data: impl Fn() -> Rc<ImageData> + 'static,
        rect: impl Fn() -> Rect + 'static,
        filter: ImageFilter,
    ) -> Self {
        Self {
            data: Box::new(data),
            rect: Box::new(rect),
            filter,
        }
    }
}

impl Component for Image {
    fn view(&self) -> View {
        let data = (self.data)();
        let rect = (self.rect)();
        View::Primitive(DrawCommand::Image {
            data,
            rect,
            filter: self.filter,
        })
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }
}
