//! The dismiss stack: which open overlay a Back gesture or an Escape key closes next.
//!
//! Separate from the overlay registry in `ui-tree`, and deliberately so. That registry orders overlays for
//! *hit-testing* and is populated when an overlay is built — `Overlay::toggleable` builds its subtree once on
//! first open and keeps it mounted, so build order says nothing about which overlay the user opened last. This
//! stack is populated on *open* and emptied on close, so its top is always the frontmost thing the user would
//! expect a dismissal to hit. It also holds only *dismissible* overlays: a tooltip or an anchored dropdown
//! never registers, so Escape never "closes" one of those instead of the dialog above it.

use std::cell::RefCell;
use std::mem::ManuallyDrop;
use std::rc::Rc;

use reactive_core::{RwSignal, detached, signal};

/// Identifies one registration, so a closing overlay can withdraw exactly its own entry.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DismissId(u64);

struct Entry {
    id: DismissId,
    dismiss: Rc<dyn Fn()>,
}

// ManuallyDrop keeps these TLS slots trivially-destructible: registering a TLS destructor from a hot-reloaded
// dylib would make dlclose unsafe (same constraint as `telar::hot_state`).
thread_local! {
    static STACK: ManuallyDrop<RefCell<Vec<Entry>>> = ManuallyDrop::new(RefCell::new(Vec::new()));
    static NEXT_ID: ManuallyDrop<RefCell<u64>> = ManuallyDrop::new(RefCell::new(0));
    // Mirrors the stack's length reactively, so a widget can style itself on whether a dialog is up (a Back
    // control that must not look disabled while it would still close something).
    static DEPTH: RwSignal<usize> = detached(|| signal(0));
}

// Republishes the stack depth. Called after every mutation, outside the stack's borrow: writing the signal can
// flush effects that read the stack.
fn publish_depth() {
    let depth = STACK.with(|s| s.borrow().len());
    DEPTH.with(|d| d.set(depth));
}

/// Registers an open overlay as the new top of the dismiss stack, returning the handle to withdraw it with.
pub fn register_dismiss(dismiss: Rc<dyn Fn()>) -> DismissId {
    let id = NEXT_ID.with(|n| {
        let mut n = n.borrow_mut();
        *n += 1;
        DismissId(*n)
    });
    STACK.with(|s| s.borrow_mut().push(Entry { id, dismiss }));
    publish_depth();
    id
}

/// Withdraws a registration, whether or not it is still the top. A no-op for an id already withdrawn.
pub fn unregister_dismiss(id: DismissId) {
    STACK.with(|s| s.borrow_mut().retain(|e| e.id != id));
    publish_depth();
}

/// Dismisses the topmost open overlay, reporting whether there was one.
///
/// The entry is removed before its handler runs: the handler sets the overlay's `open` signal false, which
/// re-runs the registering effect and would otherwise withdraw an entry this call already consumed.
pub fn dismiss_top() -> bool {
    // Release the borrow before calling out: the handler writes signals whose flush re-enters this stack.
    let Some(entry) = STACK.with(|s| s.borrow_mut().pop()) else {
        return false;
    };
    publish_depth();
    (entry.dismiss)();
    true
}

/// Reactive read of how many dismissible overlays are open — for styling an affordance on whether a dismissal
/// would do anything.
pub fn use_dismiss_depth() -> usize {
    DEPTH.with(|d| d.get())
}

/// Non-subscribing read of how many dismissible overlays are open.
pub fn dismiss_depth() -> usize {
    STACK.with(|s| s.borrow().len())
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    fn reset() {
        STACK.with(|s| s.borrow_mut().clear());
    }

    #[test]
    fn dismisses_the_most_recently_opened_first() {
        reset();
        let log = Rc::new(RefCell::new(Vec::new()));
        for name in ["dialog", "drawer"] {
            let log = log.clone();
            register_dismiss(Rc::new(move || log.borrow_mut().push(name)));
        }
        assert_eq!(dismiss_depth(), 2);

        assert!(dismiss_top());
        assert_eq!(*log.borrow(), vec!["drawer"], "the last opened goes first");
        assert!(dismiss_top());
        assert_eq!(*log.borrow(), vec!["drawer", "dialog"]);

        assert!(!dismiss_top(), "an empty stack reports nothing dismissed");
        assert_eq!(dismiss_depth(), 0);
    }

    #[test]
    fn withdrawing_out_of_order_skips_that_entry() {
        reset();
        let hit = Rc::new(Cell::new(0));
        let first = {
            let hit = hit.clone();
            register_dismiss(Rc::new(move || hit.set(hit.get() + 1)))
        };
        let log = Rc::new(RefCell::new(Vec::new()));
        {
            let log = log.clone();
            register_dismiss(Rc::new(move || log.borrow_mut().push("top")));
        }
        // The lower overlay closes on its own (its Close button, not a dismissal).
        unregister_dismiss(first);
        assert_eq!(dismiss_depth(), 1);

        assert!(dismiss_top());
        assert_eq!(*log.borrow(), vec!["top"]);
        assert_eq!(hit.get(), 0, "the withdrawn entry is never invoked");
        assert!(!dismiss_top());
    }

    #[test]
    fn escape_dismisses_only_when_nothing_holds_focus() {
        use platform_core::{Event, Key, ModifiersState, NamedKey};

        reset();
        crate::focus::clear();
        let closed = Rc::new(Cell::new(false));
        {
            let closed = closed.clone();
            register_dismiss(Rc::new(move || closed.set(true)));
        }
        let esc = Event::KeyPressed {
            key: Key::Named(NamedKey::Escape),
            modifiers: ModifiersState::default(),
        };

        // A focused editor gets first refusal: it blurs itself, and the dialog around it stays up.
        let id = crate::focus::next_id();
        crate::focus::register_as(id, crate::focus::FocusKind::Widget);
        crate::focus::request(id);
        assert_eq!(crate::dispatch_overlays(&esc), crate::EventResult::Ignored);
        assert!(!closed.get(), "the focused field consumes the first Escape");

        // Once nothing is focused, the next Escape closes the dialog.
        crate::focus::clear();
        assert_eq!(crate::dispatch_overlays(&esc), crate::EventResult::Handled);
        assert!(closed.get());
        crate::focus::unregister(id);
    }

    #[test]
    fn a_handler_that_reenters_the_stack_is_safe() {
        reset();
        // Mimics the real flow: the handler sets `open = false`, whose effect withdraws the same entry.
        let id = Rc::new(Cell::new(None::<DismissId>));
        let inner = id.clone();
        let handle = register_dismiss(Rc::new(move || {
            if let Some(i) = inner.get() {
                unregister_dismiss(i);
            }
        }));
        id.set(Some(handle));
        assert!(dismiss_top());
        assert_eq!(dismiss_depth(), 0);
    }
}
