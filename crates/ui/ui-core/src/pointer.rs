//! Pointer state and dispatch: which buttons are down, what is occluded, and how an event reaches children.

use std::cell::Cell;

use geometry_core::Rect;
use platform_core::{Event, PointerButton};
use ui_tree::EventResult;

/// Which pointer buttons are held right now.
///
/// The pointer's half of [`crate::modifiers`], and there for the same reason: a gesture that behaves one way per button has to ask, and the callbacks it is written against report *where* the pointer is, not *what* started it. A modeller is the case — drag to orbit, right-drag to pan — and widening `on_drag` to carry a button would make the whole catalogue pay for a question two widgets ask.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PointerButtons {
    pub primary: bool,
    pub secondary: bool,
    pub auxiliary: bool,
}

impl PointerButtons {
    /// Whether any button at all is down.
    pub fn any(self) -> bool {
        self.primary || self.secondary || self.auxiliary
    }

    fn slot(&mut self, button: &PointerButton) -> &mut bool {
        match button {
            PointerButton::Primary => &mut self.primary,
            PointerButton::Secondary => &mut self.secondary,
            PointerButton::Auxiliary => &mut self.auxiliary,
        }
    }

    pub(crate) fn holds(self, button: &PointerButton) -> bool {
        match button {
            PointerButton::Primary => self.primary,
            PointerButton::Secondary => self.secondary,
            PointerButton::Auxiliary => self.auxiliary,
        }
    }

    pub(crate) fn with(mut self, button: &PointerButton) -> Self {
        *self.slot(button) = true;
        self
    }
}

thread_local! {
    /// Set while a move is being dispatched into a subtree that something else is drawn over. See [`pointer_occluded`].
    static OCCLUDED: Cell<bool> = const { Cell::new(false) };
    static BUTTONS: Cell<PointerButtons> = const {
        Cell::new(PointerButtons { primary: false, secondary: false, auxiliary: false })
    };
}

/// Records what `event` says about the buttons. The runner calls this for every event before dispatch, so a handler running on this very event already sees the state it establishes.
pub fn observe_pointer(event: &Event) {
    BUTTONS.with(|b| {
        let mut held = b.get();
        match event {
            Event::PointerPressed { button, .. } => *held.slot(button) = true,
            Event::PointerReleased { button, .. } => *held.slot(button) = false,
            // A window that loses focus never sends the releases for what was down. `CursorLeft` is deliberately absent: crossing the border does not lift a button, and a live drag would lose which button started it.
            Event::FocusChanged { is_focused: false } => held = PointerButtons::default(),
            _ => return,
        }
        b.set(held);
    });
}

/// The pointer buttons held right now.
pub fn pointer_buttons() -> PointerButtons {
    BUTTONS.with(|b| b.get())
}

/// Drops the button state; parallels the other per-tree resets on teardown and hot reload.
pub fn reset_pointer() {
    BUTTONS.with(|b| b.set(PointerButtons::default()));
}

/// Whether the move being dispatched right now landed on something drawn in front of this widget.
///
/// A move is broadcast to every child, not only the one under the pointer, because a widget that armed a press or began a drag has to keep receiving them after the pointer leaves its box (pointer capture). That is right for the gesture and wrong for hover: two overlapping boxes would both read the same move as *the pointer is over me*, and a viewport would highlight the face behind the panel the user is pointing at. The container marks the covered subtrees as it broadcasts, and the widgets that track hover ask here.
pub(crate) fn pointer_occluded() -> bool {
    OCCLUDED.with(|c| c.get())
}

struct OccludedGuard(bool);

impl Drop for OccludedGuard {
    fn drop(&mut self) {
        OCCLUDED.with(|c| c.set(self.0));
    }
}

/// Marks everything dispatched until the guard drops as covered. Set-only on the way down: a subtree inside something covered is covered too, whatever its own children are stacked like.
fn occlude() -> OccludedGuard {
    OccludedGuard(OCCLUDED.with(|c| c.replace(true)))
}

pub(crate) fn pointer_coords(event: &Event) -> Option<(f64, f64)> {
    match event {
        Event::PointerMoved { x, y, .. } => Some((*x, *y)),
        Event::PointerPressed { x, y, .. } => Some((*x, *y)),
        Event::PointerReleased { x, y, .. } => Some((*x, *y)),
        Event::Scrolled { x, y, .. } => Some((*x, *y)),
        _ => None,
    }
}

/// Applies the full affine inverse of `matrix` to all pointer-coordinate events. Returns `None` for non-pointer events or when `matrix` is degenerate (det ≈ 0), so callers fall back to the original.
///
/// Public because a component that paints a subtree under a [`RenderNode::Transform`](ui_tree::RenderNode::Transform) it chose itself — a hand-placed rail or panel, rather than a laid-out one — has to put the same transform's inverse on the events it forwards there, or its hit-testing drifts from what is on screen.
pub fn transform_pointer(event: &Event, matrix: [f32; 6]) -> Option<Event> {
    let inv = geometry_core::Transform::from_array(matrix).invert()?;
    // In f64 so pointer coordinates keep their precision; `Transform::apply` would round-trip through f32.
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
                button: *button,
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
                button: *button,
                source: source.clone(),
            })
        }
        // The delta is a distance in notches or pixels, not a point in the space this maps out of. Only where the wheel turned moves with the subtree.
        Event::Scrolled { delta, x, y } => {
            let (local_x, local_y) = apply(*x, *y);
            Some(Event::Scrolled {
                delta: delta.clone(),
                x: local_x,
                y: local_y,
            })
        }
        _ => None,
    }
}

pub(crate) fn offset_pointer(event: &Event, dx: f64, dy: f64) -> Option<Event> {
    transform_pointer(event, [1.0, 0.0, 0.0, 1.0, dx as f32, dy as f32])
}

/// Returns `event` when the pointer is inside `rect`, or `None` when it is outside; non-pointer events always pass through. Callers use `None` to short-circuit to `Ignored`.
pub(crate) fn clip_pointer_event<'a>(event: &'a Event, rect: Rect) -> Option<&'a Event> {
    match pointer_coords(event) {
        Some((x, y)) if !rect.contains(x as f32, y as f32) => None,
        _ => Some(event),
    }
}

pub(crate) fn dispatch_container_event(
    children: &mut crate::layout_item::TrackedChildren,
    event: &Event,
) -> EventResult {
    let _dispatching = crate::disposal::dispatching();
    // Moves and releases broadcast to every child regardless of position, so a widget that armed a press inside its bounds still gets the release once the pointer has left (pointer capture). Each handler is guarded by its own armed state, so this never double-fires. Hit-testing below applies to presses and the wheel.
    if matches!(
        event,
        Event::PointerMoved { .. } | Event::PointerReleased { .. }
    ) {
        // The topmost child containing the point is the one the pointer is over; the others get the same move for gestures still running, but under the occlusion mark.
        let over = pointer_coords(event).and_then(|(x, y)| {
            children.iter().rposition(|c| {
                c.rect
                    .as_ref()
                    .is_some_and(|sig| sig.get().contains(x as f32, y as f32))
                    && c.item.borrow().pointer_opaque()
            })
        });
        let mut any_handled = false;
        for (i, child) in children.iter().enumerate() {
            // Covered means drawn over, so only a later sibling occludes an earlier one. Comparing for inequality also marked the children on top as covered — invisible while every sibling is opaque, and wrong the moment one is not: a `click_through` bar was told the pane beneath was shadowing it.
            let _covered = (over.is_some_and(|top| top > i)).then(occlude);
            if child.owning(|| child.item.borrow_mut().on_event(event)) == EventResult::Handled {
                any_handled = true;
            }
        }
        return if any_handled {
            EventResult::Handled
        } else {
            EventResult::Ignored
        };
    }
    let Some((x, y)) = pointer_coords(event).map(|(x, y)| (x as f32, y as f32)) else {
        return dispatch_to_children(children, event);
    };
    // Back to front, the order they are painted in: where two children overlap, the one on top takes the event whether or not it wants it. Falling sideways to a covered sibling is what made a wheel over a floating panel zoom the pane underneath. Only `absolute` makes this observable.
    for child in children.iter_mut().rev() {
        // A child with no laid-out rect cannot be hit-tested, so it is offered the event but never blocks.
        let rect = child.rect.as_ref().map(|sig| sig.get());
        // A box that misses the point may still hold one that does: an absolutely laid-out child is painted where the layout put it rather than inside its parent, so a parent of no size was painted through and hit-tested around. The miss still costs the child the right to block — only a box the point is really in covers what is behind it — and every press, drag and clip refuses a pointer outside its own rect.
        let inside = rect.is_none_or(|r| r.contains(x, y));
        let result = child.owning(|| child.item.borrow_mut().on_event(event));
        // A widget that is not there for hit-testing (an overlay, routed by its own registry) lets the search carry on to whatever it was drawn over.
        if result == EventResult::Handled
            || (inside && rect.is_some() && child.item.borrow().pointer_opaque())
        {
            return result;
        }
    }
    EventResult::Ignored
}

fn dispatch_to_children(
    children: &mut crate::layout_item::TrackedChildren,
    event: &Event,
) -> EventResult {
    for child in children.iter_mut() {
        if child.owning(|| child.item.borrow_mut().on_event(event)) == EventResult::Handled {
            return EventResult::Handled;
        }
    }
    EventResult::Ignored
}

#[cfg(test)]
#[path = "pointer_test.rs"]
mod tests;
