use std::cell::RefCell;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::rc::Rc;

use rustc_hash::FxHashSet;

pub(crate) type EffectId = usize;
pub(crate) type SignalId = usize;

pub(crate) struct EffectEntry {
    pub(crate) f: Box<dyn Fn()>,
    pub(crate) pure: bool,
    pub(crate) last_run_epoch: u64,
    pub(crate) sources: Vec<SignalId>,
    pub(crate) source_slots: Vec<usize>,
    pub(crate) source_versions: Vec<u64>,
    // Topological height; 0 = leaf (no tracked sources).
    pub(crate) height: u32,
}

pub(crate) struct SignalStorage {
    pub(crate) value: Box<dyn std::any::Any>,
    pub(crate) version: u64,
    pub(crate) subscribers: Vec<EffectId>,
    // For each subscriber: the index in that effect's `source_slots` vec that records this signal.
    pub(crate) observer_slots: Vec<usize>,
    // Handle reference count; the slab slot is freed when it reaches zero.
    pub(crate) ref_count: usize,
}

pub(crate) type OwnerId = usize;

pub(crate) struct OwnerData {
    pub(crate) effects: Vec<EffectId>,
    pub(crate) signals: Vec<SignalId>,
}

pub(crate) struct Runtime {
    pub(crate) observer_stack: Vec<EffectId>,
    pub(crate) effects: slab::Slab<EffectEntry>,
    pub(crate) signals: slab::Slab<SignalStorage>,
    pub(crate) batch_depth: usize,
    pub(crate) pending: Vec<EffectId>,
    pub(crate) memo_pending: BinaryHeap<(Reverse<u32>, EffectId)>,
    pub(crate) pending_set: FxHashSet<EffectId>,
    on_flush: Vec<(u64, Rc<dyn Fn()>)>,
    next_flush_notify_id: u64,
    pub(crate) flushing: bool,
    flush_epoch: u64,
    owner_stack: Vec<OwnerId>,
    owners: slab::Slab<OwnerData>,
}

thread_local! {
    static RUNTIME: RefCell<Runtime> = RefCell::new(Runtime {
        observer_stack: Vec::new(),
        effects: slab::Slab::new(),
        signals: slab::Slab::new(),
        batch_depth: 0,
        pending: Vec::new(),
        memo_pending: BinaryHeap::new(),
        pending_set: FxHashSet::default(),
        on_flush: Vec::new(),
        next_flush_notify_id: 0,
        flushing: false,
        flush_epoch: 0,
        owner_stack: Vec::new(),
        owners: slab::Slab::new(),
    });
}

pub struct FlushNotifyHandle {
    id: u64,
}

impl Drop for FlushNotifyHandle {
    fn drop(&mut self) {
        deregister_flush_notify(self.id);
    }
}

fn deregister_flush_notify(id: u64) {
    RUNTIME.with(|rt| {
        rt.borrow_mut()
            .on_flush
            .retain(|(entry_id, _)| *entry_id != id);
    });
}

pub fn set_flush_notify(f: impl Fn() + 'static) -> FlushNotifyHandle {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let id = rt.next_flush_notify_id;
        rt.next_flush_notify_id += 1;
        rt.on_flush.push((id, Rc::new(f)));
        FlushNotifyHandle { id }
    })
}

pub(crate) fn current_observer() -> Option<EffectId> {
    RUNTIME.with(|rt| rt.borrow().observer_stack.last().copied())
}

pub(crate) fn register_effect(f: Box<dyn Fn()>) -> EffectId {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let id = rt.effects.insert(EffectEntry {
            f,
            pure: false,
            last_run_epoch: 0,
            sources: Vec::new(),
            source_slots: Vec::new(),
            source_versions: Vec::new(),
            height: 0,
        });
        if let Some(&owner_id) = rt.owner_stack.last() {
            if rt.owners.contains(owner_id) {
                rt.owners[owner_id].effects.push(id);
            }
        }
        id
    })
}

pub(crate) fn register_pure_effect(f: Box<dyn Fn()>) -> EffectId {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let id = rt.effects.insert(EffectEntry {
            f,
            pure: true,
            last_run_epoch: 0,
            sources: Vec::new(),
            source_slots: Vec::new(),
            source_versions: Vec::new(),
            height: 0,
        });
        if let Some(&owner_id) = rt.owner_stack.last() {
            if rt.owners.contains(owner_id) {
                rt.owners[owner_id].effects.push(id);
            }
        }
        id
    })
}

pub(crate) fn create_owner() -> OwnerId {
    RUNTIME.with(|rt| {
        rt.borrow_mut().owners.insert(OwnerData {
            effects: Vec::new(),
            signals: Vec::new(),
        })
    })
}

pub(crate) fn drop_owner(id: OwnerId) {
    let owner = RUNTIME.with(|rt| rt.borrow_mut().owners.try_remove(id));
    let Some(owner) = owner else { return };
    for effect_id in owner.effects {
        deregister_effect(effect_id);
    }
    // Remove owned signals from the slab and detach them from any still-live subscribers.
    for sig_id in owner.signals {
        RUNTIME.with(|rt| {
            let mut rt = rt.borrow_mut();
            if let Some(sig) = rt.signals.try_remove(sig_id) {
                for &sub_id in &sig.subscribers {
                    if let Some(entry) = rt.effects.get_mut(sub_id) {
                        // Drop this signal from the subscriber's source lists, keeping them aligned.
                        let mut i = 0;
                        while i < entry.sources.len() {
                            if entry.sources[i] == sig_id {
                                entry.sources.swap_remove(i);
                                entry.source_slots.swap_remove(i);
                                if i < entry.source_versions.len() {
                                    entry.source_versions.swap_remove(i);
                                }
                            } else {
                                i += 1;
                            }
                        }
                    }
                }
            }
        });
    }
}

pub(crate) fn with_owner<R>(id: OwnerId, f: impl FnOnce() -> R) -> R {
    RUNTIME.with(|rt| rt.borrow_mut().owner_stack.push(id));
    let result = f();
    RUNTIME.with(|rt| rt.borrow_mut().owner_stack.pop());
    result
}

pub(crate) fn deregister_effect(id: EffectId) {
    // Clean up subscriptions first so signals don't hold dangling effect ids
    clean_effect(id);
    let removed = RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let removed = if rt.effects.contains(id) {
            Some(rt.effects.remove(id))
        } else {
            None
        };
        rt.pending_set.remove(&id);
        removed
    });
    drop(removed);
}

pub(crate) fn is_alive(id: EffectId) -> bool {
    RUNTIME.with(|rt| rt.borrow().effects.contains(id))
}

pub(crate) fn effect_height(id: EffectId) -> Option<u32> {
    RUNTIME.with(|rt| rt.borrow().effects.get(id).map(|e| e.height))
}

pub(crate) fn schedule(id: EffectId) {
    let should_flush = RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let alive = rt.effects.contains(id);
        if alive && rt.pending_set.insert(id) {
            if rt.effects[id].pure {
                let h = rt.effects[id].height;
                rt.memo_pending.push((Reverse(h), id));
            } else {
                rt.pending.push(id);
            }
        }
        rt.batch_depth == 0 && !rt.flushing
    });
    if should_flush {
        flush();
    }
}

pub(crate) fn create_signal_storage<T: 'static>(value: T, initial_ref_count: usize) -> SignalId {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let id = rt.signals.insert(SignalStorage {
            value: Box::new(value),
            version: 0,
            subscribers: Vec::new(),
            observer_slots: Vec::new(),
            ref_count: initial_ref_count,
        });
        if let Some(&owner_id) = rt.owner_stack.last() {
            if rt.owners.contains(owner_id) {
                rt.owners[owner_id].signals.push(id);
            }
        }
        id
    })
}

pub(crate) fn clone_signal(id: SignalId) {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        if rt.signals.contains(id) {
            rt.signals[id].ref_count += 1;
        }
    });
}

pub(crate) fn drop_signal(id: SignalId) {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        if !rt.signals.contains(id) {
            return;
        }
        rt.signals[id].ref_count -= 1;
        if rt.signals[id].ref_count == 0 {
            rt.signals.remove(id);
        }
    });
}

pub(crate) fn with_signal_value<T: 'static, R>(id: SignalId, f: impl FnOnce(&T) -> R) -> R {
    RUNTIME.with(|rt| {
        let rt = rt.borrow();
        let storage = &rt.signals[id];
        f(storage
            .value
            .downcast_ref::<T>()
            .expect("signal type mismatch"))
    })
}

pub(crate) fn set_signal_value<T: 'static>(id: SignalId, value: T) {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let storage = &mut rt.signals[id];
        *storage
            .value
            .downcast_mut::<T>()
            .expect("signal type mismatch") = value;
    });
}

pub(crate) fn update_signal_value<T: 'static>(id: SignalId, f: impl FnOnce(&mut T)) {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let storage = &mut rt.signals[id];
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
        if !rt.effects.contains(observer_id) {
            return;
        }
        // Already subscribed during this run.
        if rt.effects[observer_id].sources.contains(&id) {
            return;
        }
        if !rt.signals.contains(id) {
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
        if !rt.signals.contains(id) {
            return false;
        }
        rt.signals[id].version += 1;
        let subs: Vec<EffectId> = rt.signals[id].subscribers.clone();
        if subs.is_empty() {
            return false;
        }

        let mut any_scheduled = false;
        let mut dead: Vec<EffectId> = Vec::new();
        for &sub_id in &subs {
            if rt.effects.contains(sub_id) {
                if rt.pending_set.insert(sub_id) {
                    if rt.effects[sub_id].pure {
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
        if !dead.is_empty() && rt.signals.contains(id) {
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

        any_scheduled && rt.batch_depth == 0 && !rt.flushing
    });
    if should_flush {
        flush_public();
    }
}

pub(crate) fn clean_effect(id: EffectId) {
    let (sources, source_slots) = RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        if let Some(entry) = rt.effects.get_mut(id) {
            (
                std::mem::take(&mut entry.sources),
                std::mem::take(&mut entry.source_slots),
            )
        } else {
            (Vec::new(), Vec::new())
        }
    });
    for (&sig_id, &slot) in sources.iter().zip(source_slots.iter()) {
        RUNTIME.with(|rt| {
            let mut rt = rt.borrow_mut();
            if !rt.signals.contains(sig_id) {
                return;
            }
            let sig = &mut rt.signals[sig_id];
            let last = sig.subscribers.len().saturating_sub(1);
            if slot > last {
                return;
            }
            if slot == last {
                sig.subscribers.pop();
                sig.observer_slots.pop();
            } else {
                let moved_id = sig.subscribers[last];
                let moved_obs_slot = sig.observer_slots[last];
                sig.subscribers[slot] = moved_id;
                sig.observer_slots[slot] = moved_obs_slot;
                sig.subscribers.pop();
                sig.observer_slots.pop();
                if let Some(entry) = rt.effects.get_mut(moved_id) {
                    if moved_obs_slot < entry.source_slots.len() {
                        entry.source_slots[moved_obs_slot] = slot;
                    }
                }
            }
        });
    }
}

pub(crate) fn run_effect(id: EffectId) {
    // --- Step 1: epoch dedup + version check ---
    let ptr = RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        if !rt.effects.contains(id) {
            return None;
        }
        // Epoch-based dedup: skip if already ran this flush cycle
        if rt.effects[id].last_run_epoch == rt.flush_epoch && rt.flush_epoch > 0 {
            return None;
        }
        rt.effects[id].last_run_epoch = rt.flush_epoch;

        // Version check: skip the closure if every tracked source still has the version it had last run.
        let sig_ids: Vec<SignalId> = rt
            .effects
            .get(id)
            .map(|e| e.sources.clone())
            .unwrap_or_default();
        let stored_versions: Vec<u64> = rt
            .effects
            .get(id)
            .map(|e| e.source_versions.clone())
            .unwrap_or_default();
        if !sig_ids.is_empty() && sig_ids.len() == stored_versions.len() {
            let all_same = sig_ids
                .iter()
                .zip(stored_versions.iter())
                .all(|(&sig_id, &ver)| {
                    if rt.signals.contains(sig_id) {
                        rt.signals[sig_id].version == ver
                    } else {
                        false
                    }
                });
            if all_same {
                return None;
            }
        }

        // SAFETY: The slab owns this Box. No effect closure in this codebase captures its own Effect handle, so deregistration cannot happen during execution.
        let ptr: *const dyn Fn() = &*rt.effects[id].f;
        Some(ptr) // Don't push observer_stack yet; clean_effect must run first
    });
    if let Some(ptr) = ptr {
        // --- Step 2: clean stale subscriptions ---
        // Clean stale subscriptions before re-running so fresh subscriptions are tracked (Finding 1.7).
        // clean_effect acquires the runtime borrow internally (safe: we released it above).
        clean_effect(id);
        // --- Step 3: run closure with observer tracking ---
        // Push observer_stack only after cleanup so clean_effect doesn't accidentally re-register.
        RUNTIME.with(|rt| rt.borrow_mut().observer_stack.push(id));
        struct PopGuard;
        impl Drop for PopGuard {
            fn drop(&mut self) {
                RUNTIME.with(|rt| rt.borrow_mut().observer_stack.pop());
            }
        }
        let _guard = PopGuard;
        unsafe { (*ptr)() };
        drop(_guard);
        // --- Step 4: record new source versions and height ---
        RUNTIME.with(|rt| {
            let mut rt = rt.borrow_mut();
            // Collect source IDs first to avoid borrowing `effects` and `signals` simultaneously.
            let sig_ids: Vec<SignalId> = rt
                .effects
                .get(id)
                .map(|e| e.sources.clone())
                .unwrap_or_default();
            let versions: Vec<u64> = sig_ids
                .iter()
                .map(|&sig_id| {
                    if rt.signals.contains(sig_id) {
                        rt.signals[sig_id].version
                    } else {
                        0
                    }
                })
                .collect();
            // Signals are always leaf nodes (height 0), so any tracked source gives height 1.
            let new_height: u32 = if sig_ids.is_empty() { 0 } else { 1 };
            if let Some(entry) = rt.effects.get_mut(id) {
                entry.source_versions = versions;
                entry.height = new_height;
            }
        });
    }
}

pub(crate) fn flush_public() {
    flush();
}

const MAX_FLUSH_ITERATIONS: usize = 1_000;

fn flush() {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        rt.flushing = true;
        rt.flush_epoch += 1;
    });

    let should_panic = {
        let mut did_work = false;
        for _ in 0..MAX_FLUSH_ITERATIONS {
            // Drain memo_pending first (pure computations), then user effects.
            // Pop minimum height first so producers run before consumers (topological order).
            let memo_batch: Vec<EffectId> = RUNTIME.with(|rt| {
                let mut rt = rt.borrow_mut();
                let mut batch = Vec::new();
                while let Some((_, id)) = rt.memo_pending.pop() {
                    batch.push(id);
                }
                batch
            });
            let pending_batch = if memo_batch.is_empty() {
                RUNTIME.with(|rt| {
                    let mut rt = rt.borrow_mut();
                    rt.pending_set.clear();
                    std::mem::take(&mut rt.pending)
                })
            } else {
                Vec::new()
            };

            if memo_batch.is_empty() && pending_batch.is_empty() {
                RUNTIME.with(|rt| rt.borrow_mut().flushing = false);
                if did_work {
                    let cbs: smallvec::SmallVec<[Rc<dyn Fn()>; 2]> = RUNTIME.with(|rt| {
                        rt.borrow()
                            .on_flush
                            .iter()
                            .map(|(_, cb)| Rc::clone(cb))
                            .collect()
                    });
                    for cb in cbs {
                        cb();
                    }
                }
                return;
            }
            did_work = true;
            for id in memo_batch {
                run_effect(id);
            }
            for id in pending_batch {
                run_effect(id);
            }
        }
        true
    };

    RUNTIME.with(|rt| rt.borrow_mut().flushing = false);

    if should_panic {
        panic!(
            "reactive flush exceeded {MAX_FLUSH_ITERATIONS} iterations — \
             likely an effect is writing to a signal it depends on"
        );
    }
}

pub fn batch<R>(f: impl FnOnce() -> R) -> R {
    RUNTIME.with(|rt| rt.borrow_mut().batch_depth += 1);
    let result = f();
    let should_flush = RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        rt.batch_depth -= 1;
        rt.batch_depth == 0 && (!rt.pending.is_empty() || !rt.memo_pending.is_empty())
    });
    if should_flush {
        flush();
    }
    result
}

pub fn begin_batch() {
    RUNTIME.with(|rt| rt.borrow_mut().batch_depth += 1);
}

pub fn end_batch() {
    let should_flush = RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        rt.batch_depth -= 1;
        rt.batch_depth == 0 && (!rt.pending.is_empty() || !rt.memo_pending.is_empty())
    });
    if should_flush {
        flush();
    }
}
