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

const DEAD: &str = "signal read after its storage was freed";

pub(crate) fn with_signal_value<T: 'static, R>(id: SignalId, f: impl FnOnce(&T) -> R) -> R {
    RUNTIME.with(|rt| {
        let rt = rt.borrow();
        let storage = rt.signals.get(id).expect(DEAD);
        f(storage
            .value
            .downcast_ref::<T>()
            .expect("signal type mismatch"))
    })
}

pub(crate) fn set_signal_value<T: 'static>(id: SignalId, value: T) {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let storage = rt.signals.get_mut(id).expect(DEAD);
        *storage
            .value
            .downcast_mut::<T>()
            .expect("signal type mismatch") = value;
    });
}

pub(crate) fn update_signal_value<T: 'static>(id: SignalId, f: impl FnOnce(&mut T)) {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let storage = rt.signals.get_mut(id).expect(DEAD);
        f(storage
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
        // Already subscribed during this run.
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

pub(crate) fn notify_signal(id: SignalId) {
    let should_flush = RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        if !rt.signals.contains_key(id) {
            return false;
        }
        rt.signals[id].version += 1;
        // Copy subscriber ids into a reused scratch buffer rather than cloning a fresh Vec per write. We deliberately copy out (not mem::take) so the dead-subscriber cleanup below can still swap_remove from `subscribers` in place; taking it would make that cleanup operate on an empty Vec and the swap-back would clobber the removals.
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

        // Drop dead subscribers inline; we already hold the runtime borrow.
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
        // Return the buffer (with its grown capacity) for the next write to reuse.
        rt.subscriber_scratch = subs;
        should_flush
    });
    if should_flush {
        flush();
    }
}

#[cfg(test)]
mod tests {
    use slotmap::Key;

    use super::*;

    /// The hazard versioned keys exist for, written down. The arena hands the freed slot straight back, so
    /// under a raw index the id that freed it addressed whatever moved in — and when that was the same type,
    /// `with_signal_value` downcast cleanly and returned the wrong signal's value with nothing to notice.
    #[test]
    fn the_id_that_freed_a_slot_cannot_read_what_moves_into_it() {
        let scope = crate::owner_scope();
        let owner = scope.id();
        let first = create_signal_storage(1i32);
        drop(scope);
        crate::dispose_owner(owner);
        let second = create_signal_storage(2i32);

        let slot = |key: SignalId| key.data().as_ffi() as u32;
        assert_eq!(slot(first), slot(second), "the slot is reused immediately");
        assert_ne!(first, second, "but the id that freed it is not");
        assert_eq!(with_signal_value::<i32, _>(second, |v| *v), 2);
        assert!(
            RUNTIME.with(|rt| !rt.borrow().signals.contains_key(first)),
            "and the stale id resolves to nothing, though its slot is occupied"
        );
    }
}
