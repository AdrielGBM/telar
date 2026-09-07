//! The dismiss stack: which open overlay a Back gesture or an Escape key closes next.
//!
//! Separate from the overlay registry in `ui-tree`, and deliberately so. That registry orders overlays for *hit-testing* and is populated when an overlay is built — `Overlay::toggleable` builds its subtree once on first open and keeps it mounted, so build order says nothing about which overlay the user opened last. This stack is populated on *open* and emptied on close, so its top is always the frontmost thing the user would expect a dismissal to hit. It also holds only *dismissible* overlays: a tooltip or an anchored dropdown never registers, so Escape never "closes" one of those instead of the dialog above it.

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

// `ManuallyDrop` keeps these slots trivially destructible: a TLS destructor registered from a hot-reloaded dylib would make `dlclose` unsafe.
thread_local! {
    static STACK: ManuallyDrop<RefCell<Vec<Entry>>> = ManuallyDrop::new(RefCell::new(Vec::new()));
    static NEXT_ID: ManuallyDrop<RefCell<u64>> = ManuallyDrop::new(RefCell::new(0));
    // Mirrors the stack's length reactively, so a Back control can style itself on whether it would still close something.
    static DEPTH: RwSignal<usize> = detached(|| signal(0));
}

// Called after every mutation, outside the stack's borrow: writing the signal can flush effects that read it.
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
/// The entry is removed before its handler runs: the handler sets the overlay's `open` signal false, which re-runs the registering effect and would otherwise withdraw an entry this call already consumed.
pub fn dismiss_top() -> bool {
    // Release the borrow before calling out: the handler writes signals whose flush re-enters this stack.
    let Some(entry) = STACK.with(|s| s.borrow_mut().pop()) else {
        return false;
    };
    publish_depth();
    (entry.dismiss)();
    true
}

/// Reactive read of how many dismissible overlays are open — for styling an affordance on whether a dismissal would do anything.
pub fn use_dismiss_depth() -> usize {
    DEPTH.with(|d| d.get())
}

/// Non-subscribing read of how many dismissible overlays are open.
pub fn dismiss_depth() -> usize {
    STACK.with(|s| s.borrow().len())
}

#[cfg(test)]
#[path = "dismiss_test.rs"]
mod tests;
