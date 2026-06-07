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

/// Core trait for rendering and handling user input in the UI framework.
///
/// Components support two state management models:
/// - **Reactive model**: Store state in `RwSignal<T>` (from `reactive_core`). Signal changes automatically
///   trigger `view()` re-evaluation via the reactive system, enabling automatic re-renders. Prefer this
///   for most widgets to keep UI in sync with reactive state.
/// - **Imperative model**: Store state in `Cell<T>` or similar interior mutability. `view()` is called
///   each frame, but re-renders only occur when `on_event` returns `EventResult::Handled`. Use this
///   model sparingly—only when reactivity would cause excessive re-renders (e.g., high-frequency hover states).
/// Choose the reactive model for new widgets unless you have a specific performance reason to avoid it.
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
