use platform_core::Event;

use crate::view::View;

#[derive(Debug, PartialEq)]
pub enum EventResult {
    Handled,
    Ignored,
}

pub trait Component: 'static {
    fn view(&self) -> View;

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }

    fn on_mount(&self) {}

    fn on_unmount(&self) {}
}
