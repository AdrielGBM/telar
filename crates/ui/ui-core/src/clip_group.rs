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
        for child in &mut self.children {
            if child.on_event(event).is_handled() {
                return EventResult::Handled;
            }
        }
        EventResult::Ignored
    }
}

fn pointer_coords(event: &Event) -> Option<(f64, f64)> {
    match event {
        Event::PointerMoved { x, y, .. } => Some((*x, *y)),
        Event::PointerPressed { x, y, .. } => Some((*x, *y)),
        Event::PointerReleased { x, y, .. } => Some((*x, *y)),
        _ => None,
    }
}
