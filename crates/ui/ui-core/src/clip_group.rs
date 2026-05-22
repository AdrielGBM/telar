use geometry_core::Rect;
use platform_core::Event;
use ui_tree::{Component, EventResult, View};

use crate::pointer::{dispatch_to_children, pointer_coords};

pub struct ClipGroup {
    rect: Box<dyn Fn() -> Rect>,
    children: Vec<Box<dyn Component>>,
}

impl ClipGroup {
    pub fn new(rect: impl Fn() -> Rect + 'static, children: Vec<Box<dyn Component>>) -> Self {
        Self {
            rect: Box::new(rect),
            children,
        }
    }
}

impl Component for ClipGroup {
    fn view(&self) -> View {
        View::Clip {
            rect: (self.rect)(),
            children: self.children.iter().map(|c| c.view()).collect(),
        }
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        if let Some((x, y)) = pointer_coords(event) {
            if !(self.rect)().contains(x as f32, y as f32) {
                return EventResult::Ignored;
            }
        }
        dispatch_to_children(&mut self.children, event)
    }
}
