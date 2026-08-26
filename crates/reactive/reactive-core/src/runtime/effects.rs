use std::cmp::Reverse;

use super::flush::flush;
use super::owner::enter_owner;
use super::surface::current_surface;
use super::{EffectEntry, EffectId, RUNTIME, SignalId};

pub(crate) fn current_observer() -> Option<EffectId> {
    RUNTIME.with(|rt| rt.borrow().observer_stack.last().copied())
}

pub(crate) fn register_effect(f: Box<dyn Fn()>) -> EffectId {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        // Read off the stack we already hold rather than through `current_owner`, whose own borrow would collide with this one.
        let owner = rt.owner_stack.last().copied();
        let id = rt.effects.insert(EffectEntry {
            callback: f,
            surface: current_surface(),
            owner,
            is_pure: false,
            last_run_epoch: 0,
            sources: Vec::new(),
            source_slots: Vec::new(),
            source_versions: Vec::new(),
            height: 0,
            memo_dirty: false,
        });
        super::owner::attach_effect(&mut rt, id);
        id
    })
}

pub(crate) fn register_pure_effect(f: Box<dyn Fn()>) -> EffectId {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        // Read off the stack we already hold rather than through `current_owner`, whose own borrow would collide with this one.
        let owner = rt.owner_stack.last().copied();
        let id = rt.effects.insert(EffectEntry {
            callback: f,
            surface: current_surface(),
            owner,
            is_pure: true,
            last_run_epoch: 0,
            sources: Vec::new(),
            source_slots: Vec::new(),
            source_versions: Vec::new(),
            height: 0,
            memo_dirty: false,
        });
        super::owner::attach_effect(&mut rt, id);
        id
    })
}

pub(crate) fn deregister_effect(id: EffectId) {
    // Clean up subscriptions first so signals don't hold dangling effect ids
    clean_effect(id);
    let removed = RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let removed = rt.effects.remove(id);
        rt.pending_set.remove(&id);
        removed
    });
    drop(removed);
}

pub(crate) fn is_alive(id: EffectId) -> bool {
    RUNTIME.with(|rt| rt.borrow().effects.contains_key(id))
}

pub(crate) fn schedule(id: EffectId) {
    let should_flush = RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let alive = rt.effects.contains_key(id);
        if alive {
            // schedule() is only ever called by memo change notification; flag the subscriber so run_effect's signal-version shortcut cannot skip a genuinely-dirty memo read.
            rt.effects[id].memo_dirty = true;
        }
        if alive && rt.pending_set.insert(id) {
            if rt.effects[id].is_pure {
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
            if !rt.signals.contains_key(sig_id) {
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
    let ptr = RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        if !rt.effects.contains_key(id) {
            return None;
        }
        // Epoch-based dedup: skip if already ran this flush cycle
        if rt.effects[id].last_run_epoch == rt.flush_epoch && rt.flush_epoch > 0 {
            return None;
        }
        rt.effects[id].last_run_epoch = rt.flush_epoch;
        let memo_dirty = std::mem::take(&mut rt.effects[id].memo_dirty);

        // Version check: skip the closure if every tracked source still has the version it had last run. Bypassed when a memo scheduled this run — memo deps are not in `sources`, so unchanged signal versions say nothing about the memo's value.
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
        if !memo_dirty && !sig_ids.is_empty() && sig_ids.len() == stored_versions.len() {
            let all_same = sig_ids
                .iter()
                .zip(stored_versions.iter())
                .all(|(&sig_id, &ver)| {
                    if rt.signals.contains_key(sig_id) {
                        rt.signals[sig_id].version == ver
                    } else {
                        false
                    }
                });
            if all_same {
                return None;
            }
        }

        // SAFETY: The arena owns this Box. No effect closure in this codebase captures its own Effect handle, so deregistration cannot happen during execution.
        let ptr: *const dyn Fn() = &*rt.effects[id].callback;
        let surface = rt.effects[id].surface;
        let owner = rt.effects[id].owner;
        Some((ptr, surface, owner)) // Don't push observer_stack yet; clean_effect must run first
    });
    if let Some((ptr, surface, owner)) = ptr {
        // Clean stale subscriptions before re-running so fresh subscriptions are tracked (Finding 1.7). clean_effect acquires the runtime borrow internally (safe: we released it above).
        clean_effect(id);
        // Push observer_stack only after cleanup so clean_effect doesn't accidentally re-register.
        RUNTIME.with(|rt| rt.borrow_mut().observer_stack.push(id));
        struct PopGuard;
        impl Drop for PopGuard {
            fn drop(&mut self) {
                RUNTIME.with(|rt| rt.borrow_mut().observer_stack.pop());
            }
        }
        let _guard = PopGuard;
        // Owner scope: re-enter this effect's surface so its layout/overlay/focus resolve against the
        // surface that built it, even when the write that scheduled it came from another surface. The
        // guard is intentionally outside the RUNTIME borrow above (which was already released) and inert
        // for single-surface apps (surface == current surface → no-op).
        {
            let _surface_guard = surface.enter();
            // And its owner, for the same reason one step in: a re-run that starts declaring, or creates a signal it did not create the first time, has to attribute it to the scope that built this effect rather than to whatever build the flush interrupted.
            let _owner_guard = enter_owner(owner);
            unsafe { (*ptr)() };
        }
        drop(_guard);
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
                    if rt.signals.contains_key(sig_id) {
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
