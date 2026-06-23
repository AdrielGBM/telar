use geometry_core::Rect;
use platform_core::Event;
use reactive_core::RwSignal;
use ui_tree::EventResult;

pub(crate) fn pointer_coords(event: &Event) -> Option<(f64, f64)> {
    match event {
        Event::PointerMoved { x, y, .. } => Some((*x, *y)),
        Event::PointerPressed { x, y, .. } => Some((*x, *y)),
        Event::PointerReleased { x, y, .. } => Some((*x, *y)),
        _ => None,
    }
}

// Applies the full affine inverse of `m` to all pointer-coordinate events. Returns None for
// non-pointer events or when `m` is degenerate (det ≈ 0), so callers fall back to the original.
pub(crate) fn transform_pointer(event: &Event, m: [f32; 6]) -> Option<Event> {
    let [a, b, c, d, e, f] = m;
    let det = a * d - b * c;
    if det.abs() < 1e-6 {
        return None;
    }
    let inv_a = d / det;
    let inv_b = -b / det;
    let inv_c = -c / det;
    let inv_d = a / det;
    let inv_e = (c * f - d * e) / det;
    let inv_f = (b * e - a * f) / det;
    let apply = |wx: f64, wy: f64| -> (f64, f64) {
        let lx = inv_a as f64 * wx + inv_c as f64 * wy + inv_e as f64;
        let ly = inv_b as f64 * wx + inv_d as f64 * wy + inv_f as f64;
        (lx, ly)
    };
    match event {
        Event::PointerMoved { x, y, source } => {
            let (lx, ly) = apply(*x, *y);
            Some(Event::PointerMoved {
                x: lx,
                y: ly,
                source: source.clone(),
            })
        }
        Event::PointerPressed {
            x,
            y,
            button,
            source,
        } => {
            let (lx, ly) = apply(*x, *y);
            Some(Event::PointerPressed {
                x: lx,
                y: ly,
                button: button.clone(),
                source: source.clone(),
            })
        }
        Event::PointerReleased {
            x,
            y,
            button,
            source,
        } => {
            let (lx, ly) = apply(*x, *y);
            Some(Event::PointerReleased {
                x: lx,
                y: ly,
                button: button.clone(),
                source: source.clone(),
            })
        }
        _ => None,
    }
}

pub(crate) fn offset_pointer(event: &Event, dx: f64, dy: f64) -> Option<Event> {
    transform_pointer(event, [1.0, 0.0, 0.0, 1.0, dx as f32, dy as f32])
}

// Returns a reference to `event` when the pointer is inside `rect`, or None when it is outside.
// Non-pointer events always pass through (returns Some). Callers use None to short-circuit to Ignored.
pub(crate) fn clip_pointer_event<'a>(event: &'a Event, rect: Rect) -> Option<&'a Event> {
    match pointer_coords(event) {
        Some((x, y)) if !rect.contains(x as f32, y as f32) => None,
        _ => Some(event),
    }
}

// Like dispatch_to_children but skips entries for which the predicate returns false.
// For each entry, `should_dispatch` is called first — if it returns false the entry is skipped;
// then `get_result` is called to obtain the EventResult for that entry.
pub(crate) fn dispatch_to_children_filtered<T, P, D>(
    children: &mut [T],
    mut should_dispatch: P,
    mut get_result: D,
) -> EventResult
where
    P: FnMut(&T) -> bool,
    D: FnMut(&mut T) -> EventResult,
{
    for entry in children.iter_mut() {
        if !should_dispatch(entry) {
            continue;
        }
        if get_result(entry) == EventResult::Handled {
            return EventResult::Handled;
        }
    }
    EventResult::Ignored
}

pub(crate) fn dispatch_container_event(
    children: &mut Vec<(
        Box<dyn crate::layout_item::LayoutItem>,
        Option<RwSignal<Rect>>,
    )>,
    event: &Event,
) -> EventResult {
    if matches!(event, Event::PointerMoved { .. }) {
        let mut any_handled = false;
        for (child, _) in children.iter_mut() {
            if child.on_event(event) == EventResult::Handled {
                any_handled = true;
            }
        }
        return if any_handled {
            EventResult::Handled
        } else {
            EventResult::Ignored
        };
    }
    let pointer_pos = pointer_coords(event).map(|(x, y)| (x as f32, y as f32));
    dispatch_to_children_filtered(
        children,
        |(_, rect_signal)| match pointer_pos {
            Some((px, py)) => rect_signal
                .as_ref()
                .map_or(true, |sig| sig.get().contains(px, py)),
            None => true,
        },
        |(child, _)| child.on_event(event),
    )
}
