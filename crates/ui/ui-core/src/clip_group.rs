use geometry_core::Rect;
use platform_core::Event;
use ui_tree::{Component, EventResult, View};

use crate::pointer::{clip_pointer_event, dispatch_to_children};

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
        let Some(event) = clip_pointer_event(event, (self.rect)()) else {
            return EventResult::Ignored;
        };
        dispatch_to_children(&mut self.children, event)
    }
}
