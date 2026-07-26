use geometry_core::Rect;
use platform_core::Event;
use ui_tree::EventResult;

pub(crate) fn pointer_coords(event: &Event) -> Option<(f64, f64)> {
    match event {
        Event::PointerMoved { x, y, .. } => Some((*x, *y)),
        Event::PointerPressed { x, y, .. } => Some((*x, *y)),
        Event::PointerReleased { x, y, .. } => Some((*x, *y)),
        _ => None,
    }
}

/// Applies the full affine inverse of `matrix` to all pointer-coordinate events. Returns `None` for
/// non-pointer events or when `matrix` is degenerate (det ≈ 0), so callers fall back to the original.
///
/// Public because a component that paints a subtree under a [`RenderNode::Transform`] it chose itself — a
/// hand-placed rail or panel, rather than a laid-out one — has to put the same transform's inverse on the
/// events it forwards there, or its hit-testing drifts from what is on screen.
pub fn transform_pointer(event: &Event, matrix: [f32; 6]) -> Option<Event> {
    let inv = geometry_core::Transform::from_array(matrix).invert()?;
    // Map in f64 so pointer coordinates keep their precision; Transform::apply would round-trip through f32.
    let apply = |world_x: f64, world_y: f64| -> (f64, f64) {
        let local_x = inv.a as f64 * world_x + inv.c as f64 * world_y + inv.e as f64;
        let local_y = inv.b as f64 * world_x + inv.d as f64 * world_y + inv.f as f64;
        (local_x, local_y)
    };
    match event {
        Event::PointerMoved { x, y, source } => {
            let (local_x, local_y) = apply(*x, *y);
            Some(Event::PointerMoved {
                x: local_x,
                y: local_y,
                source: source.clone(),
            })
        }
        Event::PointerPressed {
            x,
            y,
            button,
            source,
        } => {
            let (local_x, local_y) = apply(*x, *y);
            Some(Event::PointerPressed {
                x: local_x,
                y: local_y,
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
            let (local_x, local_y) = apply(*x, *y);
            Some(Event::PointerReleased {
                x: local_x,
                y: local_y,
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

// Returns a reference to `event` when the pointer is inside `rect`, or None when it is outside. Non-pointer events always pass through (returns Some). Callers use None to short-circuit to Ignored.
pub(crate) fn clip_pointer_event<'a>(event: &'a Event, rect: Rect) -> Option<&'a Event> {
    match pointer_coords(event) {
        Some((x, y)) if !rect.contains(x as f32, y as f32) => None,
        _ => Some(event),
    }
}

// Like dispatch_to_children but skips entries for which the predicate returns false. For each entry, `should_dispatch` is called first — if it returns false the entry is skipped; then `get_result` is called to obtain the EventResult for that entry.
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
    children: &mut crate::layout_item::TrackedChildren,
    event: &Event,
) -> EventResult {
    // Moves AND releases broadcast to every child regardless of position: a widget that armed a press or
    // began a drag inside its bounds must still receive the release even when the pointer has since moved
    // outside (pointer-capture semantics). Each widget's release handler is guarded by its own armed/drag
    // state, so broadcasting never double-fires an unrelated widget. Position filtering (below) applies
    // only to presses, where the target is chosen by hit-test.
    if matches!(
        event,
        Event::PointerMoved { .. } | Event::PointerReleased { .. }
    ) {
        let mut any_handled = false;
        for child in children.iter() {
            if child.item.borrow_mut().on_event(event) == EventResult::Handled {
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
        |child| match pointer_pos {
            Some((pointer_x, pointer_y)) => child
                .rect
                .as_ref()
                .map_or(true, |sig| sig.get().contains(pointer_x, pointer_y)),
            None => true,
        },
        |child| child.item.borrow_mut().on_event(event),
    )
}
