//! Batching and the flush: draining scheduled effects in topological order, once per drain.

use std::cmp::Reverse;
use std::rc::Rc;

use super::effects::run_effect;
use super::{EffectId, FlushNotifyHandle, RUNTIME, SurfaceHandle};

const MAX_FLUSH_ITERATIONS: usize = 1_000;

pub(crate) fn flush() {
    // A teardown holds its tree half-applied: owners uprooted, effects still registered, signals not yet removed. An effect running now would read state its own disposal has already freed, so `dispose_owner` takes the flush back once the tree is whole again. Nor does a flush nest: the drain already running picks up whatever an effect schedules once that effect returns. A nested drain would clear `flushing` under the outer one, letting settle work run before the outer drain's effects had, and would charge its panics to the effect that happened to open it.
    let entered = RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let free = rt.disposing == 0 && !rt.flushing;
        rt.flushing |= free;
        free
    });
    if !entered {
        return;
    }

    // With the runtime shared across surfaces, a panic mid-effect must not leave `flushing` stuck true, which would wedge every surface's scheduling. The guard clears it on any exit; only the flush that raised it gets here, so clearing it restores what it was.
    struct FlushGuard;
    impl Drop for FlushGuard {
        fn drop(&mut self) {
            RUNTIME.with(|rt| rt.borrow_mut().flushing = false);
        }
    }

    let (did_work, overflowed) = {
        let _flush_guard = FlushGuard;
        let mut did_work = false;
        let mut overflowed = true;
        for _ in 0..MAX_FLUSH_ITERATIONS {
            // One epoch per drain, not one per flush. The dedup in `run_effect` makes two writes to one signal cost one run, but an effect whose source is written by a later effect in the same cascade must still run again: under a flush-wide epoch that re-run was scheduled, popped and skipped, and nothing rescheduled it. A genuine write-read cycle is still caught by the iteration cap below.
            RUNTIME.with(|rt| rt.borrow_mut().flush_epoch += 1);
            // Memos first, then user effects. Pop minimum height first, so producers run before consumers.
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
                overflowed = false;
                break;
            }
            did_work = true;
            let mut unrun = Unrun(memo_batch.into_iter().chain(pending_batch));
            for id in unrun.0.by_ref() {
                run_effect(id);
            }
        }
        (did_work, overflowed)
    };

    if overflowed {
        panic!(
            "reactive flush exceeded {MAX_FLUSH_ITERATIONS} iterations — \
             likely an effect is writing to a signal it depends on"
        );
    }

    // Notify flush observers (e.g. the runner's redraw waker) after `flushing` is cleared, so a callback that writes a signal can schedule and drive a fresh flush.
    if did_work {
        let cbs: smallvec::SmallVec<[Rc<dyn Fn()>; 2]> = RUNTIME.with(|rt| {
            rt.borrow()
                .flush_callbacks
                .iter()
                .map(|(_, cb)| Rc::clone(cb))
                .collect()
        });
        for cb in cbs {
            cb();
        }
    }
    run_settled();
}

/// The rest of a drain, put back on the queue when a panic nothing caught leaves it unrun: those effects are still dirty, and dropping them would leave their readers stale until some unrelated write happened to reschedule them.
struct Unrun<I: Iterator<Item = EffectId>>(I);

impl<I: Iterator<Item = EffectId>> Drop for Unrun<I> {
    fn drop(&mut self) {
        let rest: Vec<EffectId> = self.0.by_ref().collect();
        if rest.is_empty() {
            return;
        }
        RUNTIME.with(|rt| {
            let mut rt = rt.borrow_mut();
            for id in rest {
                let Some(entry) = rt.effects.get(id) else {
                    continue;
                };
                let (is_pure, height) = (entry.is_pure, entry.height);
                // Pushed whether or not the set already names it: a drained memo stays in the set until the user-effect drain clears it.
                rt.pending_set.insert(id);
                if is_pure {
                    rt.memo_pending.push((Reverse(height), id));
                } else {
                    rt.pending.push(id);
                }
            }
        });
    }
}

/// Defers every effect scheduled inside `f` until it returns, so a run of writes flushes once.
pub fn batch<R>(f: impl FnOnce() -> R) -> R {
    // A panic inside `f` must not leave `batch_depth` unbalanced (it would suppress every future flush on the shared runtime). The guard decrements on any exit, including an unwind.
    struct DepthGuard;
    impl Drop for DepthGuard {
        fn drop(&mut self) {
            RUNTIME.with(|rt| {
                let mut rt = rt.borrow_mut();
                rt.batch_depth = rt.batch_depth.saturating_sub(1);
            });
        }
    }
    RUNTIME.with(|rt| rt.borrow_mut().batch_depth += 1);
    let result = {
        let _depth_guard = DepthGuard;
        f()
    };
    flush_when_settled();
    result
}

/// Opens a batch. Pair with [`end_batch`]; nesting is counted.
pub fn begin_batch() {
    RUNTIME.with(|rt| rt.borrow_mut().batch_depth += 1);
}

/// Closes a batch, flushing when the outermost one closes.
pub fn end_batch() {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        debug_assert!(rt.batch_depth > 0, "end_batch without begin_batch");
        rt.batch_depth = rt.batch_depth.saturating_sub(1);
    });
    flush_when_settled();
}

/// Flushes what is pending and then runs the settle queue, unless a batch, a flush or a teardown is still open; whichever of those closes last does it instead.
pub(crate) fn flush_when_settled() {
    let due = RUNTIME.with(|rt| {
        let rt = rt.borrow();
        !rt.pending.is_empty() || !rt.memo_pending.is_empty()
    });
    if due && is_settled() {
        flush();
    }
    run_settled();
}

/// Drops every signal, effect and owner. For tests, and for a hot-reload swap.
pub fn reset_runtime() {
    RUNTIME.with(|cell| {
        let old_ptr = cell.take_ptr();
        // Install a fresh runtime BEFORE dropping the old one. Any re-entrant RUNTIME access during the old runtime's drop glue (drop_signal, etc.) will see the new empty runtime and return early — no borrow conflict, no double-free.
        if !old_ptr.is_null() {
            unsafe { drop(Box::from_raw(old_ptr)) };
        }
    });
}

/// Installs a callback run after each flush, which is how the runner learns a frame is due.
pub fn set_flush_notify(f: impl Fn() + 'static) -> FlushNotifyHandle {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let id = rt.next_flush_callback_id;
        rt.next_flush_callback_id += 1;
        rt.flush_callbacks.push((id, Rc::new(f)));
        FlushNotifyHandle { id }
    })
}

/// Runs `f` once every write made so far has reached its effects — at once if nothing is open, otherwise as the outermost batch, flush or teardown closes — for a decision that must see state an effect derives, not what it was before the write; `f` runs under the surface that was active when it was queued.
pub fn after_settle(f: impl FnOnce() + 'static) {
    if is_settled() {
        f();
        return;
    }
    let surface = super::current_surface();
    RUNTIME.with(|rt| rt.borrow_mut().settle_queue.push((surface, Box::new(f))));
}

fn is_settled() -> bool {
    RUNTIME.with(|rt| {
        let rt = rt.borrow();
        rt.batch_depth == 0 && !rt.flushing && rt.disposing == 0
    })
}

/// Runs what [`after_settle`] queued, if nothing is still holding it back.
pub(crate) fn run_settled() {
    while is_settled() {
        let queued = RUNTIME.with(|rt| std::mem::take(&mut rt.borrow_mut().settle_queue));
        if queued.is_empty() {
            return;
        }
        let mut unrun = UnrunSettled(queued.into_iter());
        for (surface, f) in unrun.0.by_ref() {
            if super::is_surface_disposed(surface) {
                continue;
            }
            let _entered = surface.enter();
            f();
        }
    }
}

type Settle = (SurfaceHandle, Box<dyn FnOnce()>);

/// What a panicking settle callback left unrun, put back at the front of the queue so the next settle point runs it, in order. Dropping it would lose work its caller was promised, with nothing to say so.
struct UnrunSettled(std::vec::IntoIter<Settle>);

impl Drop for UnrunSettled {
    fn drop(&mut self) {
        let mut rest: Vec<Settle> = self.0.by_ref().collect();
        if rest.is_empty() {
            return;
        }
        RUNTIME.with(|rt| {
            let mut rt = rt.borrow_mut();
            rest.append(&mut rt.settle_queue);
            rt.settle_queue = rest;
        });
    }
}

#[cfg(test)]
#[path = "settle_test.rs"]
mod settle_tests;
