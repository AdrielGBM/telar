use std::rc::Rc;

use super::effects::run_effect;
use super::{EffectId, FlushNotifyHandle, RUNTIME};

const MAX_FLUSH_ITERATIONS: usize = 1_000;

pub(crate) fn flush() {
    // A teardown holds its tree half-applied: owners uprooted, effects still registered, signals not yet removed. An effect that ran now would read state its own disposal has already freed, so `dispose_owner` takes the flush back once the tree is whole again.
    if RUNTIME.with(|rt| rt.borrow().disposing > 0) {
        return;
    }
    RUNTIME.with(|rt| rt.borrow_mut().flushing = true);

    // With the runtime shared across surfaces, a panic mid-effect must not leave `flushing` stuck true —
    // that would wedge every surface's scheduling (schedule() early-returns while flushing). The guard
    // clears it on any exit: normal return, the overflow panic below, or an unwind out of run_effect.
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
            // One epoch per drain, not one per flush. The dedup in `run_effect` is there so two writes to
            // the same signal in one drain cost one run; an effect whose source is written by a *later*
            // effect in the same cascade must still run again. Under a flush-wide epoch that re-run was
            // scheduled, popped and skipped, and nothing rescheduled it — the effect stayed stale until
            // some unrelated event forced it. A genuine write-read cycle is still caught by the
            // iteration cap below.
            RUNTIME.with(|rt| rt.borrow_mut().flush_epoch += 1);
            // Drain memo_pending first (pure computations), then user effects. Pop minimum height first so producers run before consumers (topological order).
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
            for id in memo_batch {
                run_effect(id);
            }
            for id in pending_batch {
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

    // Notify flush observers (e.g. the runner's redraw waker) after `flushing` is cleared, so a callback
    // that writes a signal can schedule and drive a fresh flush.
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
}

pub fn batch<R>(f: impl FnOnce() -> R) -> R {
    // A panic inside `f` must not leave `batch_depth` unbalanced (it would suppress every future flush on
    // the shared runtime). The guard decrements on any exit, including an unwind.
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
    let should_flush = RUNTIME.with(|rt| {
        let rt = rt.borrow();
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
        debug_assert!(rt.batch_depth > 0, "end_batch without begin_batch");
        rt.batch_depth = rt.batch_depth.saturating_sub(1);
        rt.batch_depth == 0 && (!rt.pending.is_empty() || !rt.memo_pending.is_empty())
    });
    if should_flush {
        flush();
    }
}

pub fn reset_runtime() {
    RUNTIME.with(|cell| {
        let old_ptr = cell.take_ptr();
        // Install a fresh runtime BEFORE dropping the old one. Any re-entrant RUNTIME access during the old runtime's drop glue (drop_signal, etc.) will see the new empty runtime and return early — no borrow conflict, no double-free.
        if !old_ptr.is_null() {
            unsafe { drop(Box::from_raw(old_ptr)) };
        }
    });
}

pub fn set_flush_notify(f: impl Fn() + 'static) -> FlushNotifyHandle {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let id = rt.next_flush_callback_id;
        rt.next_flush_callback_id += 1;
        rt.flush_callbacks.push((id, Rc::new(f)));
        FlushNotifyHandle { id }
    })
}
