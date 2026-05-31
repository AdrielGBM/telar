use platform_core::Event;

use crate::render_node::RenderNode;

#[derive(Debug, PartialEq)]
pub enum EventResult {
    Handled,
    Ignored,
}

impl EventResult {
    pub fn or(self, other: Self) -> Self {
        if matches!(self, EventResult::Handled) {
            self
        } else {
            other
        }
    }

    pub fn is_handled(&self) -> bool {
        matches!(self, EventResult::Handled)
    }
}

pub trait Component: 'static {
    fn view(&self) -> RenderNode;

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }
}

impl Component for Box<dyn Component> {
    fn view(&self) -> RenderNode {
        (**self).view()
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        (**self).on_event(event)
    }
}
