//! Keyboard focus: which widget receives key events. A base primitive with no styling of its own — a
//! focusable widget (e.g. [`crate::Input`]) requests focus on tap and consults it in `on_event`/`view`.
//!
//! Key events are broadcast to every widget (see `dispatch_container_event`), so focus is *self-filtering*:
//! a widget handles a key only when [`is_focused`] holds for its id — there is no central router. Focus is
//! a reactive signal, so a widget that reads [`current`]/[`is_focused`] inside its `view()` re-renders when
//! focus moves (e.g. to show or hide its caret). State is per-surface (each surface owns its own focus via
//! [`FocusContext`], activated by the runner), so focus never crosses windows; preserving focus across a
//! hot-reload dylib swap is out of scope.

use reactive_core::{RwSignal, signal};

/// An opaque focus identity, one per focusable widget. Allocate with [`next_id`].
pub type FocusId = u64;

/// A cheap, `Copy` handle to a focusable widget's identity, so a caller that has moved the widget into a
/// container (and no longer holds a reference to it) can still drive its focus — e.g. autofocus a hosted
/// editor when its tab activates. Obtain one from the widget (see [`crate::TextArea::focus_handle`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FocusHandle(FocusId);

impl FocusHandle {
    /// Gives focus to the handle's widget.
    pub fn request(self) {
        request(self.0);
    }

    /// Removes focus from the handle's widget, only if it currently holds it.
    pub fn release(self) {
        release(self.0);
    }

    /// Whether the handle's widget currently holds focus.
    pub fn is_focused(self) -> bool {
        is_focused(self.0)
    }
}

/// Wraps a raw [`FocusId`] in a [`FocusHandle`]. A focusable widget hands out a handle to its own id.
pub fn handle(id: FocusId) -> FocusHandle {
    FocusHandle(id)
}

/// Per-surface keyboard-focus state: the id allocator, the focused-widget signal, and the tab order.
struct FocusState {
    next_id: FocusId,
    focused: RwSignal<Option<FocusId>>,
    // Registered focusables in tab order (registration order ≈ document order). Drives Tab/Shift-Tab.
    order: Vec<FocusId>,
}

impl FocusState {
    fn new() -> Self {
        Self {
            next_id: 1,
            focused: signal(None),
            order: Vec::new(),
        }
    }
}

reactive_core::surface_local! {
    /// Per-surface focus state. The runner activates each surface's [`FocusContext`] around its
    /// build/event/frame, so focus never crosses windows.
    slot FOCUS: FocusState = FocusState::new();
    access with_focus, with_focus_ref;
    context FocusContext, FocusGuard;
}

/// The active surface's focused-widget signal, cloned out of the slot so callers never hold the slot borrow
/// across a `.set()` — its flush re-enters the slot when an effect reads [`current`].
fn focused_signal() -> RwSignal<Option<FocusId>> {
    with_focus_ref(|s| s.focused.clone())
}

/// Allocates a fresh focus id for a focusable widget.
pub fn next_id() -> FocusId {
    with_focus(|s| {
        let id = s.next_id;
        s.next_id += 1;
        id
    })
}

/// The currently focused widget, or `None`. Reactive: reading this inside a `view()` re-renders the
/// caller when focus changes.
pub fn current() -> Option<FocusId> {
    focused_signal().get()
}

/// Whether `id` currently holds focus.
pub fn is_focused(id: FocusId) -> bool {
    current() == Some(id)
}

// The three commands below `peek` the signal they write, and it matters: a command is a thing an *effect* may
// well issue ("while this row is the selected one, focus its field"), and a reactive read there would
// subscribe that effect to the focus it sets — so the next focus change anywhere would re-run it and it would
// take the focus straight back. Same rule, and the same bug, as `ScrollViewport::reveal`.

/// Gives focus to `id` (a no-op if it already holds it).
pub fn request(id: FocusId) {
    let focused = focused_signal();
    if focused.peek() != Some(id) {
        focused.set(Some(id));
    }
}

/// Removes focus from `id`, but only if it currently holds it — so a widget blurring itself never steals
/// focus away from another.
pub fn release(id: FocusId) {
    let focused = focused_signal();
    if focused.peek() == Some(id) {
        focused.set(None);
    }
}

/// Clears focus entirely, whoever holds it.
pub fn clear() {
    let focused = focused_signal();
    if focused.peek().is_some() {
        focused.set(None);
    }
}

/// Adds `id` to the tab order (at the end), if not already present. A focusable widget calls this on
/// creation; registration order is the traversal order.
pub fn register(id: FocusId) {
    with_focus(|s| {
        if !s.order.contains(&id) {
            s.order.push(id);
        }
    });
}

/// Removes `id` from the tab order and drops its focus if it held it. A focusable widget calls this on
/// drop, so a destroyed widget never lingers in traversal or as the focused id.
pub fn unregister(id: FocusId) {
    with_focus(|s| s.order.retain(|&x| x != id));
    release(id);
}

/// Moves focus to the next registered focusable in tab order (wrapping); with nothing focused, focuses
/// the first. A no-op when nothing is registered.
pub fn focus_next() {
    step(1);
}

/// Like [`focus_next`] but backwards (Shift+Tab).
pub fn focus_prev() {
    step(-1);
}

fn step(dir: isize) {
    // Snapshot the tab order and release the slot borrow before `request` (which flushes) re-enters it.
    let order = with_focus_ref(|s| s.order.clone());
    if order.is_empty() {
        return;
    }
    let n = order.len() as isize;
    let next = match current().and_then(|c| order.iter().position(|&x| x == c)) {
        Some(i) => order[((i as isize + dir).rem_euclid(n)) as usize],
        None => {
            if dir > 0 {
                order[0]
            } else {
                order[order.len() - 1]
            }
        }
    };
    request(next);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_release_and_ids_are_unique() {
        clear();
        let a = next_id();
        let b = next_id();
        assert_ne!(a, b, "ids must be unique");

        assert!(!is_focused(a));
        request(a);
        assert!(is_focused(a) && current() == Some(a));

        // Requesting b moves focus off a.
        request(b);
        assert!(is_focused(b) && !is_focused(a));

        // Releasing a (which is not focused) leaves b focused.
        release(a);
        assert!(is_focused(b));

        // Releasing the focused one clears it.
        release(b);
        assert!(current().is_none());
    }

    #[test]
    fn tab_order_steps_forward_and_back() {
        // Register three contiguous ids at the end of the order and step within that block (robust to any
        // ids other tests registered earlier on this thread).
        let (a, b, c) = (next_id(), next_id(), next_id());
        register(a);
        register(b);
        register(c);

        request(a);
        focus_next();
        assert_eq!(current(), Some(b));
        focus_next();
        assert_eq!(current(), Some(c));
        focus_prev();
        assert_eq!(current(), Some(b));

        // Unregistering the focused one drops focus and removes it from traversal.
        unregister(b);
        assert!(current().is_none());
        unregister(a);
        unregister(c);
    }
}
