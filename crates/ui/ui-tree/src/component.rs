use platform_core::Event;

use crate::view::View;

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
    fn view(&self) -> View;

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }

    fn on_mount(&self) {}

    fn on_unmount(&self) {}
}

impl Component for Box<dyn Component> {
    fn view(&self) -> View {
        (**self).view()
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        (**self).on_event(event)
    }

    fn on_mount(&self) {
        (**self).on_mount()
    }

    fn on_unmount(&self) {
        (**self).on_unmount()
    }
}
