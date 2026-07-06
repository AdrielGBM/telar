//! Priority pointer routing for overlays (portals: modals, dropdowns, toasts).
//!
//! Overlays paint on top (their draw commands are hoisted to the end at compose time, see
//! `segment.rs`), but event dispatch is an in-tree, document-order `on_event` walk. An overlay declared
//! deep in the tree would therefore be reached *late* in the walk — background content earlier in
//! document order would hit-test the same point first and steal the click, and nothing would stop a
//! press from reaching the content *behind* a modal.
//!
//! This registry closes that gap by mirroring the compose-time hoist in the event layer: an [`Overlay`]
//! registers an [`OverlaySink`], and the top-level dispatcher ([`ComponentList::on_event`]) consults the
//! registry *before* walking the tree. A positioned pointer event whose point falls inside an overlay's
//! content is dispatched to that overlay (topmost first) and consumed — so the tree walk never runs for
//! it and the content behind is blocked. A press outside every overlay falls through to the tree as
//! before, so a scrim that fills the viewport reads as a modal (blocks everything) while a small toast
//! blocks only clicks that actually land on it — the content rect *is* the barrier, no extra flag needed.
//!
//! Capture: the overlay that handles a press captures the gesture, so the following moves/releases route
//! to it regardless of where the pointer travels (a drag started in an overlay keeps tracking after the
//! pointer leaves the overlay's box), until the release.

use std::cell::RefCell;
use std::rc::Rc;

use geometry_core::Rect;
use platform_core::Event;

use crate::component::EventResult;

/// An overlay's hook into priority pointer routing. Implemented in `ui-core` by the `overlay` widget.
pub trait OverlaySink {
    /// The overlay content's current bounds, used as the hit-test barrier. A full-viewport scrim returns
    /// the whole viewport (modal); a corner toast returns just its box (blocks only clicks on itself).
    fn content_rect(&self) -> Rect;
    /// Routes a positioned pointer event into the overlay's own children (same path its in-tree
    /// `on_event` would take for non-pointer events).
    fn dispatch(&self, event: &Event) -> EventResult;
}

thread_local! {
    static OVERLAYS: RefCell<OverlayRegistry> = RefCell::new(OverlayRegistry::new());
}

struct OverlayRegistry {
    // Registered overlays in document order; the last entry is topmost (drawn on top, so hit-tested first).
    entries: Vec<(u64, Rc<dyn OverlaySink>)>,
    // The overlay that captured the current pointer gesture (set on a press it handled, cleared on release).
    captured: Option<u64>,
    next_id: u64,
}

impl OverlayRegistry {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            captured: None,
            next_id: 0,
        }
    }
}

/// Registers an overlay for priority pointer routing; returns an id to pass to [`unregister_overlay`] on
/// drop. Newly registered overlays sit on top of earlier ones.
pub fn register_overlay(sink: Rc<dyn OverlaySink>) -> u64 {
    OVERLAYS.with(|r| {
        let mut r = r.borrow_mut();
        let id = r.next_id;
        r.next_id += 1;
        r.entries.push((id, sink));
        id
    })
}

/// Removes an overlay from the registry (call from the widget's `Drop`). Also releases the pointer capture
/// if this overlay held it, so a modal dismissed mid-gesture does not leave a dangling capture.
pub fn unregister_overlay(id: u64) {
    OVERLAYS.with(|r| {
        let mut r = r.borrow_mut();
        r.entries.retain(|(entry_id, _)| *entry_id != id);
        if r.captured == Some(id) {
            r.captured = None;
        }
    });
}

fn pointer_pos(event: &Event) -> Option<(f32, f32)> {
    match event {
        Event::PointerPressed { x, y, .. }
        | Event::PointerMoved { x, y, .. }
        | Event::PointerReleased { x, y, .. } => Some((*x as f32, *y as f32)),
        _ => None,
    }
}

/// Routes a positioned pointer event to the overlay layer with priority over the main tree. Returns
/// `Handled` when an overlay consumed the event (the caller then skips the tree walk, blocking content
/// behind the overlay) and `Ignored` when it should fall through to the tree (no overlays, or the point
/// is outside every overlay and no gesture is captured). Non-pointer events always return `Ignored` so
/// keyboard and `CursorLeft` keep broadcasting through the tree.
pub fn dispatch_overlays(event: &Event) -> EventResult {
    // Snapshot the registry (cheap `Rc` clones) and drop the borrow before dispatching: a handler may
    // write signals whose deferred flush registers/unregisters an overlay, which would re-enter the borrow.
    let (entries, captured) = OVERLAYS.with(|r| {
        let r = r.borrow();
        (r.entries.clone(), r.captured)
    });
    if entries.is_empty() {
        return EventResult::Ignored;
    }
    match event {
        Event::PointerPressed { .. } => {
            let (x, y) = pointer_pos(event).unwrap();
            for (id, sink) in entries.iter().rev() {
                if sink.content_rect().contains(x, y) {
                    OVERLAYS.with(|r| r.borrow_mut().captured = Some(*id));
                    sink.dispatch(event);
                    return EventResult::Handled;
                }
            }
            EventResult::Ignored
        }
        Event::PointerMoved { .. } | Event::PointerReleased { .. } => {
            let is_release = matches!(event, Event::PointerReleased { .. });
            if let Some(cap_id) = captured {
                if let Some((_, sink)) = entries.iter().find(|(id, _)| *id == cap_id) {
                    sink.dispatch(event);
                    if is_release {
                        OVERLAYS.with(|r| r.borrow_mut().captured = None);
                    }
                    return EventResult::Handled;
                }
                // The capturing overlay is gone (dismissed mid-gesture); drop the stale capture.
                OVERLAYS.with(|r| r.borrow_mut().captured = None);
            }
            let (x, y) = pointer_pos(event).unwrap();
            for (_, sink) in entries.iter().rev() {
                if sink.content_rect().contains(x, y) {
                    sink.dispatch(event);
                    return EventResult::Handled;
                }
            }
            EventResult::Ignored
        }
        _ => EventResult::Ignored,
    }
}

#[cfg(test)]
fn reset() {
    OVERLAYS.with(|r| *r.borrow_mut() = OverlayRegistry::new());
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use platform_core::{PointerButton, PointerSource};

    use super::*;

    struct RecordingSink {
        rect: Rect,
        hits: Rc<Cell<u32>>,
    }

    impl OverlaySink for RecordingSink {
        fn content_rect(&self) -> Rect {
            self.rect
        }
        fn dispatch(&self, _event: &Event) -> EventResult {
            self.hits.set(self.hits.get() + 1);
            EventResult::Handled
        }
    }

    fn sink(rect: Rect) -> (Rc<dyn OverlaySink>, Rc<Cell<u32>>) {
        let hits = Rc::new(Cell::new(0));
        let sink: Rc<dyn OverlaySink> = Rc::new(RecordingSink {
            rect,
            hits: Rc::clone(&hits),
        });
        (sink, hits)
    }

    fn press(x: f64, y: f64) -> Event {
        Event::PointerPressed {
            x,
            y,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        }
    }
    fn moved(x: f64, y: f64) -> Event {
        Event::PointerMoved {
            x,
            y,
            source: PointerSource::Mouse,
        }
    }
    fn released(x: f64, y: f64) -> Event {
        Event::PointerReleased {
            x,
            y,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        }
    }

    #[test]
    fn no_overlays_falls_through() {
        reset();
        assert_eq!(dispatch_overlays(&press(10.0, 10.0)), EventResult::Ignored);
    }

    #[test]
    fn press_inside_is_consumed_outside_falls_through() {
        reset();
        let (s, hits) = sink(Rect::new(0.0, 0.0, 100.0, 100.0));
        let id = register_overlay(s);

        // Inside the overlay: consumed (blocks the content behind) and delivered to the sink.
        assert_eq!(dispatch_overlays(&press(50.0, 50.0)), EventResult::Handled);
        assert_eq!(hits.get(), 1);
        // Release ends the gesture. Outside the overlay: falls through to the tree, sink untouched.
        dispatch_overlays(&released(50.0, 50.0));
        assert_eq!(
            dispatch_overlays(&press(500.0, 500.0)),
            EventResult::Ignored
        );

        unregister_overlay(id);
    }

    #[test]
    fn topmost_overlay_wins() {
        reset();
        let (bottom, bottom_hits) = sink(Rect::new(0.0, 0.0, 100.0, 100.0));
        let (top, top_hits) = sink(Rect::new(0.0, 0.0, 100.0, 100.0));
        let b = register_overlay(bottom);
        let t = register_overlay(top);

        dispatch_overlays(&press(50.0, 50.0));
        assert_eq!(
            top_hits.get(),
            1,
            "topmost (last registered) receives the press"
        );
        assert_eq!(
            bottom_hits.get(),
            0,
            "the overlay below must not also get it"
        );

        unregister_overlay(t);
        unregister_overlay(b);
    }

    #[test]
    fn capture_routes_moves_and_release_even_outside() {
        reset();
        let (s, hits) = sink(Rect::new(0.0, 0.0, 100.0, 100.0));
        let id = register_overlay(s);

        // Press inside captures the gesture.
        assert_eq!(dispatch_overlays(&press(50.0, 50.0)), EventResult::Handled);
        // A move that leaves the overlay still routes to it (a drag started inside keeps tracking).
        assert_eq!(
            dispatch_overlays(&moved(500.0, 500.0)),
            EventResult::Handled
        );
        // The release, also outside, reaches the overlay and ends the capture.
        assert_eq!(
            dispatch_overlays(&released(500.0, 500.0)),
            EventResult::Handled
        );
        assert_eq!(hits.get(), 3);
        // After release, an outside press falls through again.
        assert_eq!(
            dispatch_overlays(&press(500.0, 500.0)),
            EventResult::Ignored
        );

        unregister_overlay(id);
    }

    #[test]
    fn unregister_stops_routing_and_clears_capture() {
        reset();
        let (s, _hits) = sink(Rect::new(0.0, 0.0, 100.0, 100.0));
        let id = register_overlay(s);
        // Capture a gesture, then unregister (as a dismissed modal would on drop) before the release.
        dispatch_overlays(&press(50.0, 50.0));
        unregister_overlay(id);
        // With no overlays left, everything falls through and no stale capture lingers.
        assert_eq!(
            dispatch_overlays(&released(50.0, 50.0)),
            EventResult::Ignored
        );
        assert_eq!(dispatch_overlays(&press(50.0, 50.0)), EventResult::Ignored);
    }
}
