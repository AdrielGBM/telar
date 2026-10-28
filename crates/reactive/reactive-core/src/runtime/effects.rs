//! Effect registration, scheduling, subscription cleanup, memo heights, and the version and epoch checks that decide whether a scheduled effect actually has to run.

use std::cmp::Reverse;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use super::owner::deliver_panic;

use super::surface::current_surface;
use super::{EffectEntry, EffectId, RUNTIME, Runtime, SignalId};

/// Runs `f` with no observer, so nothing it reads subscribes whichever effect is running.
pub(crate) fn untracked<R>(f: impl FnOnce() -> R) -> R {
    struct Restore(Vec<EffectId>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let hidden = std::mem::take(&mut self.0);
            RUNTIME.with(|rt| rt.borrow_mut().observer_stack = hidden);
        }
    }
    let _restore = Restore(RUNTIME.with(|rt| std::mem::take(&mut rt.borrow_mut().observer_stack)));
    f()
}

pub(crate) fn register_effect(f: Box<dyn Fn()>) -> EffectId {
    insert_effect(f, None)
}

pub(crate) fn register_memo_effect(f: Box<dyn Fn()>, memo: SignalId) -> EffectId {
    insert_effect(f, Some(memo))
}

fn insert_effect(f: Box<dyn Fn()>, produces: Option<SignalId>) -> EffectId {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        // Resolved off the borrow already held: `current_owner` takes its own, which would collide.
        let owner = super::owner::owning_id(&mut rt);
        let id = rt.effects.insert(EffectEntry {
            callback: Rc::from(f),
            surface: current_surface(),
            owner,
            produces,
            last_run_epoch: 0,
            sources: Vec::new(),
            source_slots: Vec::new(),
            source_versions: Vec::new(),
            height: 0,
        });
        if let Some(storage) = produces.and_then(|memo| rt.signals.get_mut(memo)) {
            storage.producer = Some(id);
        }
        super::owner::attach_effect(&mut rt, id);
        id
    })
}

pub(crate) fn deregister_effect(id: EffectId) {
    // Subscriptions first, so signals do not hold dangling effect ids.
    clean_effect(id);
    let removed = RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let removed = rt.effects.remove(id);
        rt.pending_set.remove(&id);
        removed
    });
    drop(removed);
}

/// Queues again a memo recompute that could not run, flushing at once when `now` and nothing holds the flush back; `now` is false during an unwind, where running effects could panic again.
pub(crate) fn reschedule_memo(id: EffectId, now: bool) {
    let should_flush = RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        if !rt.effects.contains_key(id) {
            return false;
        }
        rt.enqueue(id);
        now && rt.batch_depth == 0 && !rt.flushing
    });
    if should_flush {
        super::flush::flush();
    }
}

impl Runtime {
    pub(crate) fn enqueue(&mut self, id: EffectId) {
        let Some(entry) = self.effects.get(id) else {
            return;
        };
        if !self.pending_set.insert(id) {
            return;
        }
        if entry.produces.is_some() {
            self.memo_pending.push((Reverse(entry.height), id));
        } else {
            self.pending.push(id);
        }
    }

    fn set_height(&mut self, id: EffectId, height: u32) {
        let Some(entry) = self.effects.get_mut(id) else {
            return;
        };
        if entry.height == height {
            return;
        }
        entry.height = height;
        // The entry queued under the old height is skipped when it surfaces, so a pending memo needs one under the new.
        if entry.produces.is_some() && self.pending_set.contains(&id) {
            self.memo_pending.push((Reverse(height), id));
        }
    }

    fn raise_dependents(&mut self, id: EffectId) {
        // An acyclic graph has no height above its effect count, so a raise past it is going round a cycle.
        let ceiling = u32::try_from(self.effects.len()).unwrap_or(u32::MAX);
        let mut raised = vec![id];
        while let Some(id) = raised.pop() {
            let Some(entry) = self.effects.get(id) else {
                continue;
            };
            let floor = entry.height.saturating_add(1);
            let Some(storage) = entry.produces.and_then(|memo| self.signals.get(memo)) else {
                continue;
            };
            if floor > ceiling {
                continue;
            }
            let start = raised.len();
            raised.extend(
                storage
                    .subscribers
                    .iter()
                    .copied()
                    .filter(|&sub| self.effects.get(sub).is_some_and(|e| e.height < floor)),
            );
            for &sub in &raised[start..] {
                self.set_height(sub, floor);
            }
        }
    }

    fn record_run(&mut self, id: EffectId) {
        let Some(entry) = self.effects.get_mut(id) else {
            return;
        };
        let mut versions = std::mem::take(&mut entry.source_versions);
        versions.clear();
        let entry = &self.effects[id];
        let mut height = 0;
        for &source in &entry.sources {
            let storage = self.signals.get(source);
            versions.push(storage.map_or(0, |s| s.version));
            let above = storage
                .and_then(|s| s.producer)
                .and_then(|producer| self.effects.get(producer))
                .map_or(1, |producer| producer.height.saturating_add(1));
            height = height.max(above);
        }
        let previous = entry.height;
        self.effects[id].source_versions = versions;
        self.set_height(id, height);
        if height > previous {
            self.raise_dependents(id);
        }
    }

    fn sources_unchanged(&self, entry: &EffectEntry) -> bool {
        !entry.sources.is_empty()
            && entry.sources.len() == entry.source_versions.len()
            && entry
                .sources
                .iter()
                .zip(&entry.source_versions)
                .all(|(&source, &version)| {
                    self.signals
                        .get(source)
                        .is_some_and(|s| s.version == version)
                })
    }
}

pub(crate) fn clean_effect(id: EffectId) {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let rt = &mut *rt;
        let Some(entry) = rt.effects.get_mut(id) else {
            return;
        };
        let mut sources = std::mem::take(&mut entry.sources);
        let mut source_slots = std::mem::take(&mut entry.source_slots);
        for (&sig_id, &slot) in sources.iter().zip(source_slots.iter()) {
            let Some(sig) = rt.signals.get_mut(sig_id) else {
                continue;
            };
            let last = sig.subscribers.len().saturating_sub(1);
            if slot > last {
                continue;
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
                if let Some(entry) = rt.effects.get_mut(moved_id)
                    && moved_obs_slot < entry.source_slots.len()
                {
                    entry.source_slots[moved_obs_slot] = slot;
                }
            }
        }
        sources.clear();
        source_slots.clear();
        if let Some(entry) = rt.effects.get_mut(id) {
            entry.sources = sources;
            entry.source_slots = source_slots;
        }
    });
}

/// Runs `id` now whatever its epoch and versions say: for a memo read that finds no value, because the computation that would have left one panicked and nothing has moved since to schedule another.
pub(crate) fn force_run_effect(id: EffectId) {
    run(id, true);
}

pub(crate) fn run_effect(id: EffectId) {
    run(id, false);
}

fn run(id: EffectId, forced: bool) {
    let prepared = RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let epoch = rt.flush_epoch;
        let entry = rt.effects.get_mut(id)?;
        if !forced && entry.last_run_epoch == epoch && epoch > 0 {
            return None;
        }
        entry.last_run_epoch = epoch;
        let entry = &rt.effects[id];
        if !forced && rt.sources_unchanged(entry) {
            return None;
        }
        Some((Rc::clone(&entry.callback), entry.surface, entry.owner))
    });
    let Some((callback, surface, owner)) = prepared else {
        return;
    };
    // Before re-running, so fresh subscriptions are tracked.
    clean_effect(id);
    // After cleanup, so `clean_effect` does not re-register. The owner rides along in the same borrow: a re-run that starts declaring must attribute it to the scope that built this effect, and a borrow of its own for that cost 3-5% of a whole-tree re-run.
    let owner_depth = RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        rt.observer_stack.push(id);
        rt.running_effects += 1;
        let depth = rt.owner_stack.len();
        if let Some(owner) = owner
            && rt.owners.contains_key(owner)
        {
            rt.owner_stack.push(owner);
        }
        depth
    });
    struct PopGuard(usize);
    impl Drop for PopGuard {
        fn drop(&mut self) {
            RUNTIME.with(|rt| {
                let mut rt = rt.borrow_mut();
                rt.observer_stack.pop();
                rt.running_effects -= 1;
                rt.owner_stack.truncate(self.0);
            });
        }
    }
    let _guard = PopGuard(owner_depth);
    // Re-enter this effect's surface, so its layout, overlay and focus resolve against the surface that built it even when the write that scheduled it came from another. Outside the runtime borrow, and inert for a single-surface app.
    let outcome = surface.with_entered(|| catch_unwind(AssertUnwindSafe(|| callback())));
    drop(_guard);
    RUNTIME.with(|rt| rt.borrow_mut().record_run(id));
    if let Err(payload) = outcome {
        deliver_panic(owner, payload);
    }
}
