//! Keyboard focus: which widget receives key events. A base primitive with no styling of its own — a
//! focusable widget (e.g. [`crate::Input`]) requests focus on tap and consults it in `on_event`/`view`.
//!
//! Key events are broadcast to every widget (see `dispatch_container_event`), so focus is *self-filtering*:
//! a widget handles a key only when [`is_focused`] holds for its id — there is no central router. Focus is
//! a reactive signal, so a widget that reads [`current`]/[`is_focused`] inside its `view()` re-renders when
//! focus moves (e.g. to show or hide its caret). State is thread-local, matching the layout runtime;
//! preserving focus across a hot-reload dylib swap is out of scope.

use std::cell::{Cell, RefCell};

use reactive_core::{RwSignal, signal};

/// An opaque focus identity, one per focusable widget. Allocate with [`next_id`].
pub type FocusId = u64;

thread_local! {
    static NEXT_ID: Cell<FocusId> = const { Cell::new(1) };
    static FOCUSED: RwSignal<Option<FocusId>> = signal(None);
    // Registered focusables in tab order (registration order ≈ document order). Drives Tab/Shift-Tab.
    static ORDER: RefCell<Vec<FocusId>> = const { RefCell::new(Vec::new()) };
}

/// Allocates a fresh focus id for a focusable widget.
pub fn next_id() -> FocusId {
    NEXT_ID.with(|c| {
        let id = c.get();
        c.set(id + 1);
        id
    })
}

/// The currently focused widget, or `None`. Reactive: reading this inside a `view()` re-renders the
/// caller when focus changes.
pub fn current() -> Option<FocusId> {
    FOCUSED.with(|f| f.get())
}

/// Whether `id` currently holds focus.
pub fn is_focused(id: FocusId) -> bool {
    current() == Some(id)
}

/// Gives focus to `id` (a no-op if it already holds it).
pub fn request(id: FocusId) {
    FOCUSED.with(|f| {
        if f.get() != Some(id) {
            f.set(Some(id));
        }
    });
}

/// Removes focus from `id`, but only if it currently holds it — so a widget blurring itself never steals
/// focus away from another.
pub fn release(id: FocusId) {
    FOCUSED.with(|f| {
        if f.get() == Some(id) {
            f.set(None);
        }
    });
}

/// Clears focus entirely, whoever holds it.
pub fn clear() {
    FOCUSED.with(|f| {
        if f.get().is_some() {
            f.set(None);
        }
    });
}

/// Adds `id` to the tab order (at the end), if not already present. A focusable widget calls this on
/// creation; registration order is the traversal order.
pub fn register(id: FocusId) {
    ORDER.with_borrow_mut(|o| {
        if !o.contains(&id) {
            o.push(id);
        }
    });
}

/// Removes `id` from the tab order and drops its focus if it held it. A focusable widget calls this on
/// drop, so a destroyed widget never lingers in traversal or as the focused id.
pub fn unregister(id: FocusId) {
    ORDER.with_borrow_mut(|o| o.retain(|&x| x != id));
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
    let next = ORDER.with_borrow(|o| {
        if o.is_empty() {
            return None;
        }
        let n = o.len() as isize;
        match current().and_then(|c| o.iter().position(|&x| x == c)) {
            Some(i) => {
                let ni = ((i as isize + dir).rem_euclid(n)) as usize;
                Some(o[ni])
            }
            None => Some(if dir > 0 { o[0] } else { o[o.len() - 1] }),
        }
    });
    if let Some(id) = next {
        request(id);
    }
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
