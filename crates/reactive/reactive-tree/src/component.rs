use platform_core::Event;

use crate::view::View;

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

pub(crate) trait AnyComponent {
    fn view(&self) -> View;
    fn on_event(&mut self, event: &Event) -> EventResult;
    #[allow(dead_code)]
    fn on_mount(&self);
    fn on_unmount(&self);
}

impl<C: Component> AnyComponent for C {
    fn view(&self) -> View {
        Component::view(self)
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        Component::on_event(self, event)
    }

    fn on_mount(&self) {
        Component::on_mount(self)
    }

    fn on_unmount(&self) {
        Component::on_unmount(self)
    }
}
