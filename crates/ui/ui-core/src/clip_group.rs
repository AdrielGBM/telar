use platform_core::Event;
use renderer_core::Rect;
use ui_tree::{Component, EventResult, View};

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

    pub fn static_rect(rect: Rect, children: Vec<Box<dyn Component>>) -> Self {
        Self {
            rect: Box::new(move || rect),
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
        for child in &mut self.children {
            if child.on_event(event) == EventResult::Handled {
                return EventResult::Handled;
            }
        }
        EventResult::Ignored
    }
}
