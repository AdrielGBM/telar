//! Priority pointer routing for overlays (portals: modals, dropdowns, toasts).
//!
//! Overlays paint on top (their draw commands are hoisted to the end at compose time, see `segment.rs`), but event dispatch is an in-tree, document-order `on_event` walk. An overlay declared deep in the tree would therefore be reached *late* in the walk — background content earlier in document order would hit-test the same point first and steal the click, and nothing would stop a press from reaching the content *behind* a modal.
//!
//! This registry closes that gap by mirroring the compose-time hoist in the event layer: an [`Overlay`] registers an [`OverlaySink`], and the top-level dispatcher ([`ComponentList::on_event`]) consults the registry *before* walking the tree. A positioned pointer event whose point falls inside an overlay's content is dispatched to that overlay (topmost first) and consumed — so the tree walk never runs for it and the content behind is blocked. A press outside every overlay falls through to the tree as before, so a scrim that fills the viewport reads as a modal (blocks everything) while a small toast blocks only clicks that actually land on it — the content rect is the coarse barrier.
//!
//! Click-through: an overlay may opt out of blocking ([`OverlaySink::blocking`] = false). Then, even for a point inside its content rect, the event is consumed only when one of its children actually handles it; otherwise it falls through to the overlays/tree behind. This is how a full-viewport toast or tooltip layer stays non-modal — clicks on its transparent area reach the page, only its visible panel captures.
//!
//! Capture: the overlay that handles a press captures the gesture, so the following moves/releases route to it regardless of where the pointer travels (a drag started in an overlay keeps tracking after the pointer leaves the overlay's box), until the release.

use std::rc::Rc;

use geometry_core::Rect;
use platform_core::Event;

use crate::component::EventResult;

/// An overlay's hook into priority pointer routing. Implemented in `ui-core` by the `overlay` widget.
pub trait OverlaySink {
    /// The overlay content's current bounds, used as the hit-test barrier. A full-viewport scrim returns the whole viewport (modal); a corner toast or an anchored dropdown returns just its box (blocks only clicks on itself).
    fn content_rect(&self) -> Rect;
    /// Routes a positioned pointer event into the overlay's own children (same path its in-tree `on_event` would take for non-pointer events). Returns `Handled` when a child consumed it.
    fn dispatch(&self, event: &Event) -> EventResult;
    /// Whether the overlay swallows every pointer event inside its [`content_rect`](Self::content_rect) (a modal, the default) or only those a child actually handled (a click-through toast/tooltip layer, so clicks on its transparent area fall through to the content behind).
    fn blocking(&self) -> bool {
        true
    }
}

reactive_core::surface_local! {
    /// A per-surface overlay registry: the modals/toasts/tooltips registered for priority pointer routing on this surface. The runner activates each surface's [`OverlayContext`] around its build/event/frame.
    slot OVERLAYS: OverlayRegistry = OverlayRegistry::new();
    access with_overlays, with_overlays_ref;
    context OverlayContext, OverlayGuard;
}

struct OverlayRegistry {
    // In document order; the last entry is topmost, so hit-tested first.
    entries: Vec<(u64, Rc<dyn OverlaySink>)>,
    // Set on a press it handled, cleared on release.
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

/// Registers an overlay for priority pointer routing; returns an id to pass to [`unregister_overlay`] on drop. Newly registered overlays sit on top of earlier ones.
pub fn register_overlay(sink: Rc<dyn OverlaySink>) -> u64 {
    with_overlays(|r| {
        let id = r.next_id;
        r.next_id += 1;
        r.entries.push((id, sink));
        id
    })
}

/// Removes an overlay from the registry (call from the widget's `Drop`). Also releases the pointer capture if this overlay held it, so a modal dismissed mid-gesture does not leave a dangling capture.
pub fn unregister_overlay(id: u64) {
    with_overlays(|r| {
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

/// Routes a positioned pointer event to the overlay layer with priority over the main tree. Returns `Handled` when an overlay consumed the event (the caller then skips the tree walk, blocking content behind the overlay) and `Ignored` when it should fall through to the tree (no overlays, or the point is outside every overlay and no gesture is captured). Non-pointer events always return `Ignored` so keyboard and `CursorLeft` keep broadcasting through the tree.
pub fn dispatch_overlays(event: &Event) -> EventResult {
    let press = matches!(event, Event::PointerPressed { .. });
    let release = matches!(event, Event::PointerReleased { .. });
    // Only these three reach an overlay, and the snapshot below is not free: taking it for a key press cloned the whole registry to answer `Ignored`.
    if !press && !release && !matches!(event, Event::PointerMoved { .. }) {
        return EventResult::Ignored;
    }
    // Snapshot and drop the borrow before dispatching: a handler may write signals whose deferred flush registers or unregisters an overlay, which would re-enter the borrow.
    let (entries, captured) = with_overlays_ref(|r| (r.entries.clone(), r.captured));
    if entries.is_empty() {
        return EventResult::Ignored;
    }
    let (x, y) = pointer_pos(event).unwrap();

    // A gesture that began on an overlay stays there wherever the pointer goes, until it is released.
    if !press && let Some(cap_id) = captured {
        if let Some((_, sink)) = entries.iter().find(|(id, _)| *id == cap_id) {
            sink.dispatch(event);
            if release {
                with_overlays(|r| r.captured = None);
            }
            return EventResult::Handled;
        }
        // The capturing overlay was dismissed mid-gesture, so drop the stale capture.
        with_overlays(|r| r.captured = None);
    }

    // Topmost first. A modal consumes the event over its whole barrier; a click-through overlay only where a child took it, otherwise the walk continues below and ultimately to the tree.
    for (id, sink) in entries.iter().rev() {
        if !sink.content_rect().contains(x, y) {
            continue;
        }
        let handled = sink.dispatch(event) == EventResult::Handled;
        if sink.blocking() || handled {
            if press {
                // So following moves and releases route here wherever the pointer goes.
                with_overlays(|r| r.captured = Some(*id));
            }
            return EventResult::Handled;
        }
    }
    EventResult::Ignored
}

#[cfg(test)]
fn reset() {
    with_overlays(|r| *r = OverlayRegistry::new());
}

#[cfg(test)]
#[path = "overlay_dispatch_test.rs"]
mod tests;
