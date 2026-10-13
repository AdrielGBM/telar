//! Kept apart from `ui-tree`'s overlay registry, which follows build order — a toggleable overlay is built once and kept — while Escape, Back and Enter must reach the dismissible overlay the user opened last.
//!
//! Per surface, like the overlays it closes: Escape pressed in one window, or in the tree around a nested surface, must not close a dialog open somewhere else.

use std::cell::Cell;
use std::rc::Rc;

use layout_core::NodeId;
use reactive_core::{Effect, RwSignal, Transaction, effect, on_cleanup, signal};

use crate::surface_context::HomeSurface;

/// Identifies one registration, so a closing overlay can withdraw exactly its own entry.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DismissId(u64);

struct Entry {
    id: DismissId,
    /// The box the overlay is declared in; while it or an ancestor is out of layout flow the entry is skipped.
    within: Option<NodeId>,
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

reactive_core::dropless_thread_local! {
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

fn push(within: Option<NodeId>, dismiss: Rc<dyn Fn()>, confirm: Option<Rc<dyn Fn()>>) -> DismissId {
    let id = NEXT_ID.with(|n| {
        n.set(n.get() + 1);
        DismissId(n.get())
    });
    with_stack(|s| {
        s.entries.push(Entry {
            id,
            within,
            dismiss,
            confirm,
        })
    });
    publish_depth();
    id
}

/// Registers an open overlay as the new top of the active surface's dismiss stack, returning the handle to withdraw it with.
pub fn register_dismiss(dismiss: Rc<dyn Fn()>) -> DismissId {
    push(None, dismiss, None)
}

/// Withdraws a registration from the active surface's stack, whether or not it is still the top. A no-op for an id already withdrawn.
pub fn unregister_dismiss(id: DismissId) {
    with_stack(|s| s.entries.retain(|e| e.id != id));
    publish_depth();
}

/// A registration withdrawn when dropped, held by the effect that registers on open so disposing the overlay's owner while it is open withdraws the entry with the effect's closure.
pub struct DismissRegistration {
    id: DismissId,
    home: HomeSurface,
}

impl DismissRegistration {
    pub fn new(dismiss: Rc<dyn Fn()>) -> Self {
        Self::registered(push(None, dismiss, None))
    }

    /// A registration for an overlay declared in the box `within`: Escape and Back skip it while that box or any ancestor is out of layout flow, as inside a parked [`Movable`](crate::Movable), and reach it again once it is shown.
    pub fn within(within: NodeId, dismiss: Rc<dyn Fn()>) -> Self {
        Self::registered(push(Some(within), dismiss, None))
    }

    /// A registration Enter can close too, through `confirm`, while it is the top. See [`confirm_top`].
    pub fn confirmable(dismiss: Rc<dyn Fn()>, confirm: Rc<dyn Fn()>) -> Self {
        Self::registered(push(None, dismiss, Some(confirm)))
    }

    /// [`confirmable`](Self::confirmable) for an overlay declared in the box `within`; see [`within`](Self::within).
    pub fn confirmable_within(
        within: NodeId,
        dismiss: Rc<dyn Fn()>,
        confirm: Rc<dyn Fn()>,
    ) -> Self {
        Self::registered(push(Some(within), dismiss, Some(confirm)))
    }

    fn from_parts(within: Option<NodeId>, dismiss: Rc<dyn Fn()>, confirm: Rc<dyn Fn()>) -> Self {
        Self::registered(push(within, dismiss, Some(confirm)))
    }

    fn registered(id: DismissId) -> Self {
        Self {
            id,
            home: HomeSurface::here(),
        }
    }
}

impl Drop for DismissRegistration {
    fn drop(&mut self) {
        let Some(_home) = self.home.enter() else {
            return;
        };
        unregister_dismiss(self.id);
    }
}

/// The topmost entry that is not inside a hidden box.
fn effective_top() -> Option<DismissId> {
    let placed: Vec<(DismissId, Option<NodeId>)> =
        with_stack_ref(|s| s.entries.iter().map(|e| (e.id, e.within)).collect());
    placed
        .into_iter()
        .rev()
        .find(|(_, within)| !within.is_some_and(crate::input_region::is_hidden))
        .map(|(id, _)| id)
}

fn take(id: DismissId) -> Option<Entry> {
    with_stack(|s| {
        let at = s.entries.iter().position(|e| e.id == id)?;
        Some(s.entries.remove(at))
    })
}

/// Dismisses the topmost open overlay of the active surface that is shown, reporting whether there was one. An overlay inside a hidden box stays registered, and is reachable again when the box is shown.
///
/// The entry is removed before its handler runs: the handler sets the overlay's `open` signal false, which re-runs the registering effect and would otherwise withdraw an entry this call already consumed.
pub fn dismiss_top() -> bool {
    // Release the borrow before calling out: the handler writes signals whose flush re-enters this stack.
    let Some(entry) = effective_top().and_then(take) else {
        return false;
    };
    publish_depth();
    (entry.dismiss)();
    true
}

/// Confirms the topmost shown overlay when it takes a confirmation, reporting whether it did; a top with none answers `false` and the key goes on to the tree.
pub fn confirm_top() -> bool {
    let top = effective_top();
    let takes_confirm = top.is_some_and(|id| {
        with_stack_ref(|s| s.entries.iter().any(|e| e.id == id && e.confirm.is_some()))
    });
    let Some(confirm) = top
        .filter(|_| takes_confirm)
        .and_then(take)
        .and_then(|e| e.confirm)
    else {
        return false;
    };
    publish_depth();
    confirm();
    true
}

/// Reactive read of how many dismissible overlays are open and shown on the active surface — for styling an affordance on whether a dismissal would do anything.
pub fn use_dismiss_depth() -> usize {
    with_stack_ref(|s| s.depth).get();
    shown_depth(crate::input_region::tracks_hidden)
}

/// Non-subscribing read of how many dismissible overlays are open and shown on the active surface.
pub fn dismiss_depth() -> usize {
    shown_depth(crate::input_region::is_hidden)
}

fn shown_depth(hidden: impl Fn(NodeId) -> bool) -> usize {
    let within: Vec<Option<NodeId>> =
        with_stack_ref(|s| s.entries.iter().map(|e| e.within).collect());
    within
        .into_iter()
        .filter(|within| !within.is_some_and(&hidden))
        .count()
}

/// Puts an overlay that edits a value on the dismiss stack with the one commit convention every popover shares: opening begins `transaction`, Escape/Back reverts and closes, and every other way of closing (Enter, an outside click, a Done button) closes and commits; joining a `transaction` already open elsewhere instead leaves closing to decide nothing. Call it under the owner the overlay lives in, in place of [`DismissRegistration`].
pub fn register_transaction<T: Clone + 'static>(
    open: RwSignal<bool>,
    transaction: Transaction<T>,
) -> Effect {
    register_transaction_in(None, open, transaction)
}

/// [`register_transaction`] for an overlay declared in the box `within`; see [`DismissRegistration::within`].
pub fn register_transaction_within<T: Clone + 'static>(
    within: NodeId,
    open: RwSignal<bool>,
    transaction: Transaction<T>,
) -> Effect {
    register_transaction_in(Some(within), open, transaction)
}

fn register_transaction_in<T: Clone + 'static>(
    within: Option<NodeId>,
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
                DismissRegistration::from_parts(within, cancel.clone(), confirm.clone())
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
