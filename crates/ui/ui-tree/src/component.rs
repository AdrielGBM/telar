//! [`Component`]: the one trait every widget implements — render a view, answer an event.

use platform_core::Event;

use crate::render_node::RenderNode;

#[derive(Debug, PartialEq)]
/// Whether a widget consumed an event, or let it carry on to whatever is behind it.
pub enum EventResult {
    Handled,
    Ignored,
}

/// Imperative-state components re-render only when `on_event` returns `EventResult::Handled`; reactive-state components re-render automatically on signal change.
pub trait Component: 'static {
    fn view(&self) -> RenderNode;

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }

    /// Human-readable widget type name for the devtools tree inspector.
    fn debug_name(&self) -> &'static str {
        "Component"
    }
}

impl Component for Box<dyn Component> {
    fn view(&self) -> RenderNode {
        (**self).view()
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        (**self).on_event(event)
    }

    fn debug_name(&self) -> &'static str {
        (**self).debug_name()
    }
}
