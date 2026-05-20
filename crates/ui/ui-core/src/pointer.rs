use platform_core::Event;
use ui_tree::{Component, EventResult};

pub(crate) fn pointer_coords(event: &Event) -> Option<(f64, f64)> {
    match event {
        Event::PointerMoved { x, y, .. } => Some((*x, *y)),
        Event::PointerPressed { x, y, .. } => Some((*x, *y)),
        Event::PointerReleased { x, y, .. } => Some((*x, *y)),
        _ => None,
    }
}

pub(crate) fn offset_pointer(event: &Event, dx: f64, dy: f64) -> Option<Event> {
    match event {
        Event::PointerMoved { x, y, source } => Some(Event::PointerMoved {
            x: x + dx,
            y: y + dy,
            source: source.clone(),
        }),
        Event::PointerPressed {
            x,
            y,
            button,
            source,
        } => Some(Event::PointerPressed {
            x: x + dx,
            y: y + dy,
            button: button.clone(),
            source: source.clone(),
        }),
        Event::PointerReleased {
            x,
            y,
            button,
            source,
        } => Some(Event::PointerReleased {
            x: x + dx,
            y: y + dy,
            button: button.clone(),
            source: source.clone(),
        }),
        _ => None,
    }
}

pub(crate) fn dispatch_to_children(
    children: &mut [Box<dyn Component>],
    event: &Event,
) -> EventResult {
    for child in children {
        if child.on_event(event).is_handled() {
            return EventResult::Handled;
        }
    }
    EventResult::Ignored
}
