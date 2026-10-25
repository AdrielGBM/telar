//! Signal storage, and what a handle that outlived it gets.
//!
//! The three answers differ on purpose, and this is the one place they are written down.
//!
//! **A read panics.** There is no value to return and no honest stand-in: a default would travel through a layout as if it were real, and a wrong number in a reactive UI is far more expensive to find than a stack trace. The message names the type the caller expected, which usually identifies the handle on its own.
//!
//! **A write panics, except during a teardown.** Outside one it is the same lifetime error as a dead read. Inside one the storage is not missing but destroyed, deliberately, by the pass that is running — a cleanup writing a signal a sibling root already freed is doing meaningless work rather than wrong work, and there is no subscriber left to deliver it to. See [`super::owner::dispose_owner`].
//!
//! **Tracking is silent.** Subscribing to a dead signal is bookkeeping with nothing to record; no value is involved, so nothing can be got wrong by doing nothing.
//!
//! A caller that legitimately holds a handle longer than its owner — one kept in a store the tree does not own — should not be finding this out by crashing. [`crate::RwSignal::is_alive`] and the `try_*` reads are there to be asked first, and [`crate::detached`] is there so the question does not arise.
//!
//! **A value is borrowed on its own, never with the runtime**, so the closure given to `.with` / `.update` may read and write any other signal, while touching the same one in a way that would alias panics naming its type and both call sites.

use std::any::Any;
use std::cell::{Ref, RefMut};
use std::cmp::Reverse;
use std::panic::Location;
use std::rc::Rc;

use super::flush::flush;
use super::site_cell::{HeldAt, SiteCell};
use super::{EffectId, RUNTIME, SignalId, SignalStorage};

struct SignalCell<T>(SiteCell<T>);

impl<T: 'static> SignalCell<T> {
    #[track_caller]
    fn read(&self) -> Ref<'_, T> {
        match self.0.try_read() {
            Ok(value) => value,
            Err(held) => reentered::<T>("read", &held),
        }
    }

    #[track_caller]
    fn write(&self) -> RefMut<'_, T> {
        match self.0.try_write() {
            Ok(value) => value,
            Err(held) => reentered::<T>("written", &held),
        }
    }
}

#[cold]
#[track_caller]
fn reentered<T>(action: &str, held: &HeldAt) -> ! {
    let type_name = std::any::type_name::<T>();
    let here = Location::caller();
    panic!(
        "signal of type {type_name} {action} inside its own `.with` / `.update`.\n  \
         wanted by:  {here}\n  \
         last taken: {held}\n\
         A signal cannot be written while one of its own `.with` / `.update` closures is running, nor read while \
         its own `.update` is: copy what you need out of the closure and touch the signal after it returns."
    )
}

fn insert_storage(value: Rc<dyn Any>) -> SignalId {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let id = rt.signals.insert(SignalStorage {
            value,
            version: 0,
            subscribers: Vec::new(),
            observer_slots: Vec::new(),
        });
        super::owner::attach_signal(&mut rt, id);
        id
    })
}

pub(crate) fn create_signal_storage<T: 'static>(value: T) -> SignalId {
    insert_storage(Rc::new(SignalCell(SiteCell::new(value))))
}

/// A slot holding `value` as it is, for a node that keeps its state behind a cell of its own and only needs the arena to own it.
pub(crate) fn create_shared_storage<V: 'static>(value: Rc<V>) -> SignalId {
    insert_storage(value)
}

/// Names the type the handle expected, because the id alone identifies nothing a reader can act on.
fn dead(action: &str, type_name: &'static str) -> String {
    format!("signal of type {type_name} {action} after its storage was freed")
}

fn downcast<V: 'static>(value: Rc<dyn Any>) -> Rc<V> {
    value.downcast::<V>().expect("signal type mismatch")
}

fn stored<V: 'static>(id: SignalId) -> Option<Rc<V>> {
    RUNTIME
        .with(|rt| rt.borrow().signals.get(id).map(|s| Rc::clone(&s.value)))
        .map(downcast)
}

/// [`stored`] for a write, answering `None` for storage a teardown in flight has freed.
fn stored_for_write<T: 'static>(id: SignalId, action: &str) -> Option<Rc<SignalCell<T>>> {
    let found = RUNTIME.with(|rt| {
        let rt = rt.borrow();
        match rt.signals.get(id) {
            Some(storage) => Ok(Rc::clone(&storage.value)),
            None => Err(rt.disposing > 0),
        }
    });
    match found {
        Ok(value) => Some(downcast(value)),
        // A teardown destroying what a cleanup then writes to is the shape of the pass, not a mistake in it.
        Err(true) => None,
        Err(false) => panic!("{}", dead(action, std::any::type_name::<T>())),
    }
}

/// The value [`create_shared_storage`] stored.
pub(crate) fn shared_storage<V: 'static>(id: SignalId) -> Rc<V> {
    stored(id).unwrap_or_else(|| panic!("{}", dead("read", std::any::type_name::<V>())))
}

pub(crate) fn signal_is_alive(id: SignalId) -> bool {
    RUNTIME.with(|rt| rt.borrow().signals.contains_key(id))
}

#[track_caller]
pub(crate) fn with_signal_value<T: 'static, R>(id: SignalId, f: impl FnOnce(&T) -> R) -> R {
    let Some(cell) = stored::<SignalCell<T>>(id) else {
        panic!("{}", dead("read", std::any::type_name::<T>()));
    };
    f(&cell.read())
}

/// [`with_signal_value`] for a caller that would rather ask than crash.
#[track_caller]
pub(crate) fn try_with_signal_value<T: 'static, R>(
    id: SignalId,
    f: impl FnOnce(&T) -> R,
) -> Option<R> {
    let cell = stored::<SignalCell<T>>(id)?;
    Some(f(&cell.read()))
}

#[track_caller]
pub(crate) fn set_signal_value<T: 'static>(id: SignalId, value: T) {
    let Some(cell) = stored_for_write::<T>(id, "written") else {
        return;
    };
    // Dropped once the borrow has ended, so a drop that reaches this signal finds it settled rather than collides.
    let previous = std::mem::replace(&mut *cell.write(), value);
    drop(previous);
}

#[track_caller]
pub(crate) fn update_signal_value<T: 'static>(id: SignalId, f: impl FnOnce(&mut T)) {
    if let Some(cell) = stored_for_write::<T>(id, "updated") {
        f(&mut cell.write());
    }
}

/// [`update_signal_value`] that answers `None` for freed storage and never notifies, for state a signal's subscribers must not hear about.
#[track_caller]
pub(crate) fn try_update_signal_value<T: 'static, R>(
    id: SignalId,
    f: impl FnOnce(&mut T) -> R,
) -> Option<R> {
    let cell = stored::<SignalCell<T>>(id)?;
    Some(f(&mut cell.write()))
}

/// How many writes the signal has seen, or `None` once its storage is gone.
pub(crate) fn signal_version(id: SignalId) -> Option<u64> {
    RUNTIME.with(|rt| rt.borrow().signals.get(id).map(|s| s.version))
}

/// Whether a live effect or memo is subscribed to `id`. A subscriber disposed since is not counted, though its id may stay in the list until the next write prunes it.
pub(crate) fn signal_has_subscribers(id: SignalId) -> bool {
    RUNTIME.with(|rt| {
        let rt = rt.borrow();
        rt.signals
            .get(id)
            .is_some_and(|s| s.subscribers.iter().any(|e| rt.effects.contains_key(*e)))
    })
}

/// Marks `id` as held by a transaction, answering `false` when one already holds it.
pub(crate) fn claim_transaction(id: SignalId) -> bool {
    RUNTIME.with(|rt| rt.borrow_mut().transactions.insert(id))
}

pub(crate) fn release_transaction(id: SignalId) {
    RUNTIME.with(|rt| rt.borrow_mut().transactions.remove(&id));
}

pub(crate) fn track_signal(id: SignalId) {
    RUNTIME.with(|rt| {
        let Some(observer_id) = rt.borrow().observer_stack.last().copied() else {
            return;
        };
        let mut rt = rt.borrow_mut();
        if !rt.effects.contains_key(observer_id) {
            return;
        }
        if rt.effects[observer_id].sources.contains(&id) {
            return;
        }
        if !rt.signals.contains_key(id) {
            return;
        }
        let sub_slot = rt.signals[id].subscribers.len();
        rt.signals[id].subscribers.push(observer_id);
        rt.signals[id].observer_slots.push(0);

        let source_idx = rt.effects[observer_id].sources.len();
        rt.effects[observer_id].sources.push(id);
        rt.effects[observer_id].source_slots.push(sub_slot);

        rt.signals[id].observer_slots[sub_slot] = source_idx;
    });
}

/// Says so when the effect doing the writing is one this signal wakes, which is a loop with no end: a write bumps the version whether or not the value changed, so the effect schedules itself and arrives here again.
///
/// The read is what has to change, not the write — [`crate::RwSignal::peek`] is the one that does not subscribe. Debug builds only, and once per pair: this fires from inside the loop it is reporting.
#[cfg(debug_assertions)]
fn self_waking(rt: &mut super::Runtime, id: SignalId) -> bool {
    let Some(&observer) = rt.observer_stack.last() else {
        return false;
    };
    if !rt.signals[id].subscribers.contains(&observer) {
        return false;
    }
    rt.self_waking.insert((observer, id))
}

pub(crate) fn notify_signal(id: SignalId) {
    let should_flush = RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        if !rt.signals.contains_key(id) {
            return false;
        }
        rt.signals[id].version += 1;
        #[cfg(debug_assertions)]
        if self_waking(&mut rt, id) {
            tracing::warn!(
                "an effect is writing a signal it subscribed to, so this write wakes it and it writes again — read it with `peek` where the effect doing the writing is the one reading"
            );
        }
        // Copied into a reused scratch buffer rather than cloning a fresh Vec per write. Deliberately copied, not `mem::take`n, so the dead-subscriber cleanup below can still `swap_remove` in place.
        let mut subs = std::mem::take(&mut rt.subscriber_scratch);
        subs.clear();
        subs.extend_from_slice(&rt.signals[id].subscribers);
        if subs.is_empty() {
            rt.subscriber_scratch = subs;
            return false;
        }

        let mut any_scheduled = false;
        let mut dead: Vec<EffectId> = Vec::new();
        for &sub_id in &subs {
            if rt.effects.contains_key(sub_id) {
                if rt.pending_set.insert(sub_id) {
                    if rt.effects[sub_id].is_pure {
                        let h = rt.effects[sub_id].height;
                        rt.memo_pending.push((Reverse(h), sub_id));
                    } else {
                        rt.pending.push(sub_id);
                    }
                }
                any_scheduled = true;
            } else {
                dead.push(sub_id);
            }
        }

        if !dead.is_empty() && rt.signals.contains_key(id) {
            let sig = &mut rt.signals[id];
            let mut i = 0;
            while i < sig.subscribers.len() {
                if dead.contains(&sig.subscribers[i]) {
                    sig.subscribers.swap_remove(i);
                    sig.observer_slots.swap_remove(i);
                } else {
                    i += 1;
                }
            }
        }

        let should_flush = any_scheduled && rt.batch_depth == 0 && !rt.flushing;
        rt.subscriber_scratch = subs;
        should_flush
    });
    if should_flush {
        flush();
    }
}

#[cfg(test)]
#[path = "signals_test.rs"]
mod tests;
