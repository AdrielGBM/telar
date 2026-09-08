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

use std::cmp::Reverse;

use super::flush::flush;
use super::{EffectId, RUNTIME, SignalId, SignalStorage};

pub(crate) fn create_signal_storage<T: 'static>(value: T) -> SignalId {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let id = rt.signals.insert(SignalStorage {
            value: Box::new(value),
            version: 0,
            subscribers: Vec::new(),
            observer_slots: Vec::new(),
        });
        super::owner::attach_signal(&mut rt, id);
        id
    })
}

/// Names the type the handle expected, because the id alone identifies nothing a reader can act on.
fn dead(action: &str, type_name: &'static str) -> String {
    format!("signal of type {type_name} {action} after its storage was freed")
}

pub(crate) fn signal_is_alive(id: SignalId) -> bool {
    RUNTIME.with(|rt| rt.borrow().signals.contains_key(id))
}

pub(crate) fn with_signal_value<T: 'static, R>(id: SignalId, f: impl FnOnce(&T) -> R) -> R {
    RUNTIME.with(|rt| {
        let rt = rt.borrow();
        let storage = rt
            .signals
            .get(id)
            .unwrap_or_else(|| panic!("{}", dead("read", std::any::type_name::<T>())));
        f(storage
            .value
            .downcast_ref::<T>()
            .expect("signal type mismatch"))
    })
}

/// [`with_signal_value`] for a caller that would rather ask than crash.
pub(crate) fn try_with_signal_value<T: 'static, R>(
    id: SignalId,
    f: impl FnOnce(&T) -> R,
) -> Option<R> {
    RUNTIME.with(|rt| {
        let rt = rt.borrow();
        let storage = rt.signals.get(id)?;
        Some(f(storage
            .value
            .downcast_ref::<T>()
            .expect("signal type mismatch")))
    })
}

pub(crate) fn set_signal_value<T: 'static>(id: SignalId, value: T) {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        if !rt.signals.contains_key(id) {
            // A teardown destroying what a cleanup then writes to is the shape of the pass, not a mistake in it.
            if rt.disposing > 0 {
                return;
            }
            panic!("{}", dead("written", std::any::type_name::<T>()));
        }
        *rt.signals[id]
            .value
            .downcast_mut::<T>()
            .expect("signal type mismatch") = value;
    });
}

pub(crate) fn update_signal_value<T: 'static>(id: SignalId, f: impl FnOnce(&mut T)) {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        if !rt.signals.contains_key(id) {
            if rt.disposing > 0 {
                return;
            }
            panic!("{}", dead("updated", std::any::type_name::<T>()));
        }
        f(rt.signals[id]
            .value
            .downcast_mut::<T>()
            .expect("signal type mismatch"));
    });
}

pub(crate) fn track_signal(id: SignalId) {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let observer_id = match rt.observer_stack.last().copied() {
            Some(id) => id,
            None => return,
        };
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
