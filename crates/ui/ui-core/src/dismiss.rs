//! Kept apart from `ui-tree`'s overlay registry, which follows build order — a toggleable overlay is built once and kept — while Escape, Back and Enter must reach the dismissible overlay the user opened last.

use std::cell::{Cell, RefCell};
use std::mem::ManuallyDrop;
use std::rc::Rc;

use reactive_core::{Effect, RwSignal, Transaction, detached, effect, on_cleanup, signal};

/// Identifies one registration, so a closing overlay can withdraw exactly its own entry.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DismissId(u64);

struct Entry {
    id: DismissId,
    dismiss: Rc<dyn Fn()>,
    confirm: Option<Rc<dyn Fn()>>,
}

// `ManuallyDrop` keeps these slots trivially destructible: a TLS destructor registered from a hot-reloaded dylib would make `dlclose` unsafe.
thread_local! {
    static STACK: ManuallyDrop<RefCell<Vec<Entry>>> =
        const { ManuallyDrop::new(RefCell::new(Vec::new())) };
    static NEXT_ID: ManuallyDrop<RefCell<u64>> = const { ManuallyDrop::new(RefCell::new(0)) };
    // Mirrors the stack's length reactively, so a Back control can style itself on whether it would still close something.
    static DEPTH: RwSignal<usize> = detached(|| signal(0));
}

// Called after every mutation, outside the stack's borrow: writing the signal can flush effects that read it.
fn publish_depth() {
    let depth = STACK.with(|s| s.borrow().len());
    // A registration dropped with the arenas of a runtime being reset reaches here after the mirror's storage went with them.
    DEPTH.with(|d| {
        if d.is_alive() {
            d.set(depth);
        }
    });
}

fn push(dismiss: Rc<dyn Fn()>, confirm: Option<Rc<dyn Fn()>>) -> DismissId {
    let id = NEXT_ID.with(|n| {
        let mut n = n.borrow_mut();
        *n += 1;
        DismissId(*n)
    });
    STACK.with(|s| {
        s.borrow_mut().push(Entry {
            id,
            dismiss,
            confirm,
        })
    });
    publish_depth();
    id
}

/// Registers an open overlay as the new top of the dismiss stack, returning the handle to withdraw it with.
pub fn register_dismiss(dismiss: Rc<dyn Fn()>) -> DismissId {
    push(dismiss, None)
}

/// Withdraws a registration, whether or not it is still the top. A no-op for an id already withdrawn.
pub fn unregister_dismiss(id: DismissId) {
    STACK.with(|s| s.borrow_mut().retain(|e| e.id != id));
    publish_depth();
}

/// A registration withdrawn when dropped, held by the effect that registers on open so disposing the overlay's owner while it is open withdraws the entry with the effect's closure.
pub struct DismissRegistration(DismissId);

impl DismissRegistration {
    pub fn new(dismiss: Rc<dyn Fn()>) -> Self {
        Self(register_dismiss(dismiss))
    }

    /// A registration Enter can close too, through `confirm`, while it is the top. See [`confirm_top`].
    pub fn confirmable(dismiss: Rc<dyn Fn()>, confirm: Rc<dyn Fn()>) -> Self {
        Self(push(dismiss, Some(confirm)))
    }
}

impl Drop for DismissRegistration {
    fn drop(&mut self) {
        unregister_dismiss(self.0);
    }
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

/// Confirms the topmost open overlay when it takes a confirmation, reporting whether it did; a top with none answers `false` and the key goes on to the tree.
pub fn confirm_top() -> bool {
    let entry = STACK.with(|s| {
        let mut s = s.borrow_mut();
        if s.last().is_some_and(|e| e.confirm.is_some()) {
            s.pop()
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

/// Reactive read of how many dismissible overlays are open — for styling an affordance on whether a dismissal would do anything.
pub fn use_dismiss_depth() -> usize {
    DEPTH.with(|d| d.get())
}

/// Non-subscribing read of how many dismissible overlays are open.
pub fn dismiss_depth() -> usize {
    STACK.with(|s| s.borrow().len())
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
