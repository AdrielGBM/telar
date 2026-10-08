//! Kept apart from `ui-tree`'s overlay registry, which follows build order — a toggleable overlay is built once and kept — while Escape, Back and Enter must reach the dismissible overlay the user opened last.
//!
//! Per surface, like the overlays it closes: Escape pressed in one window, or in the tree around a nested surface, must not close a dialog open somewhere else.

use std::cell::Cell;
use std::rc::Rc;

use reactive_core::{Effect, RwSignal, SurfaceHandle, Transaction, effect, on_cleanup, signal};

/// Identifies one registration, so a closing overlay can withdraw exactly its own entry.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DismissId(u64);

struct Entry {
    id: DismissId,
    dismiss: Rc<dyn Fn()>,
    confirm: Option<Rc<dyn Fn()>>,
}

struct Stack {
    entries: Vec<Entry>,
    // Mirrors the stack's length reactively, so a Back control can style itself on whether it would still close something.
    depth: RwSignal<usize>,
}

reactive_core::surface_local! {
    /// Per-surface dismiss stack: Escape, Back and Enter answer to the overlays of the surface they were pressed in.
    slot STACK: Stack = Stack { entries: Vec::new(), depth: signal(0) };
    access with_stack, with_stack_ref;
    context DismissContext, DismissGuard;
}

thread_local! {
    // Per thread rather than per surface, so an id names one registration wherever it is withdrawn from.
    static NEXT_ID: Cell<u64> = const { Cell::new(0) };
}

// Called after every mutation, outside the stack's borrow: writing the signal can flush effects that read it.
fn publish_depth() {
    let (depth, mirror) = with_stack_ref(|s| (s.entries.len(), s.depth));
    // A registration dropped with the arenas of a runtime being reset reaches here after the mirror's storage went with them.
    if mirror.is_alive() {
        mirror.set(depth);
    }
}

fn push(dismiss: Rc<dyn Fn()>, confirm: Option<Rc<dyn Fn()>>) -> DismissId {
    let id = NEXT_ID.with(|n| {
        n.set(n.get() + 1);
        DismissId(n.get())
    });
    with_stack(|s| {
        s.entries.push(Entry {
            id,
            dismiss,
            confirm,
        })
    });
    publish_depth();
    id
}

/// Registers an open overlay as the new top of the active surface's dismiss stack, returning the handle to withdraw it with.
pub fn register_dismiss(dismiss: Rc<dyn Fn()>) -> DismissId {
    push(dismiss, None)
}

/// Withdraws a registration from the active surface's stack, whether or not it is still the top. A no-op for an id already withdrawn.
pub fn unregister_dismiss(id: DismissId) {
    with_stack(|s| s.entries.retain(|e| e.id != id));
    publish_depth();
}

/// A registration withdrawn when dropped, held by the effect that registers on open so disposing the overlay's owner while it is open withdraws the entry with the effect's closure.
pub struct DismissRegistration {
    id: DismissId,
    surface: SurfaceHandle,
}

impl DismissRegistration {
    pub fn new(dismiss: Rc<dyn Fn()>) -> Self {
        Self::registered(register_dismiss(dismiss))
    }

    /// A registration Enter can close too, through `confirm`, while it is the top. See [`confirm_top`].
    pub fn confirmable(dismiss: Rc<dyn Fn()>, confirm: Rc<dyn Fn()>) -> Self {
        Self::registered(push(dismiss, Some(confirm)))
    }

    fn registered(id: DismissId) -> Self {
        Self {
            id,
            surface: reactive_core::current_surface(),
        }
    }
}

impl Drop for DismissRegistration {
    fn drop(&mut self) {
        let _entered = self.surface.enter();
        // A surface torn down first took its stack with it, and the one active now is not this registration's.
        if reactive_core::current_surface() != self.surface {
            return;
        }
        unregister_dismiss(self.id);
    }
}

/// Dismisses the topmost open overlay of the active surface, reporting whether there was one.
///
/// The entry is removed before its handler runs: the handler sets the overlay's `open` signal false, which re-runs the registering effect and would otherwise withdraw an entry this call already consumed.
pub fn dismiss_top() -> bool {
    // Release the borrow before calling out: the handler writes signals whose flush re-enters this stack.
    let Some(entry) = with_stack(|s| s.entries.pop()) else {
        return false;
    };
    publish_depth();
    (entry.dismiss)();
    true
}

/// Confirms the topmost open overlay when it takes a confirmation, reporting whether it did; a top with none answers `false` and the key goes on to the tree.
pub fn confirm_top() -> bool {
    let entry = with_stack(|s| {
        if s.entries.last().is_some_and(|e| e.confirm.is_some()) {
            s.entries.pop()
        } else {
            None
        }
    });
    let Some(confirm) = entry.and_then(|e| e.confirm) else {
        return false;
    };
    publish_depth();
    confirm();
    true
}

/// Reactive read of how many dismissible overlays are open on the active surface — for styling an affordance on whether a dismissal would do anything.
pub fn use_dismiss_depth() -> usize {
    with_stack_ref(|s| s.depth).get()
}

/// Non-subscribing read of how many dismissible overlays are open on the active surface.
pub fn dismiss_depth() -> usize {
    with_stack_ref(|s| s.entries.len())
}

/// Puts an overlay that edits a value on the dismiss stack with the one commit convention every popover shares: opening begins `transaction`, Escape/Back reverts and closes, and every other way of closing (Enter, an outside click, a Done button) closes and commits; joining a `transaction` already open elsewhere instead leaves closing to decide nothing. Call it under the owner the overlay lives in, in place of [`DismissRegistration`].
pub fn register_transaction<T: Clone + 'static>(
    open: RwSignal<bool>,
    transaction: Transaction<T>,
) -> Effect {
    let holding = Rc::new(Cell::new(false));
    {
        let holding = holding.clone();
        on_cleanup(move || {
            if holding.replace(false) {
                let _ = transaction.revert();
            }
        });
    }
    let cancel: Rc<dyn Fn()> = {
        let holding = holding.clone();
        Rc::new(move || {
            if holding.replace(false) {
                let _ = transaction.revert();
            }
            open.set(false);
        })
    };
    let confirm: Rc<dyn Fn()> = Rc::new(move || open.set(false));
    let registered: Cell<Option<DismissRegistration>> = Cell::new(None);
    effect(move || {
        if open.get() {
            let held = registered.take().unwrap_or_else(|| {
                holding.set(transaction.begin().is_ok());
                DismissRegistration::confirmable(cancel.clone(), confirm.clone())
            });
            registered.set(Some(held));
        } else if let Some(held) = registered.take() {
            drop(held);
            if holding.replace(false) {
                let _ = transaction.commit();
            }
        }
    })
}

#[cfg(test)]
#[path = "dismiss_test.rs"]
mod tests;
