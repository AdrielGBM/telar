use std::cell::RefCell;
use std::rc::Rc;

use super::effects::run_effect;
use super::{EffectId, FlushNotifyHandle, RUNTIME, Runtime};

const MAX_FLUSH_ITERATIONS: usize = 1_000;

pub(crate) fn flush() {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        rt.flushing = true;
        rt.flush_epoch += 1;
    });

    let should_panic = {
        let mut did_work = false;
        for _ in 0..MAX_FLUSH_ITERATIONS {
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
                RUNTIME.with(|rt| rt.borrow_mut().flushing = false);
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

pub fn reset_runtime() {
    RUNTIME.with(|cell| {
        let old_ptr = cell.0.get();
        // Install a fresh runtime BEFORE dropping the old one. Any re-entrant RUNTIME access during the old runtime's drop glue (drop_signal, etc.) will see the new empty runtime and return early — no borrow conflict, no double-free.
        cell.0
            .set(Box::into_raw(Box::new(RefCell::new(Runtime::new()))));
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
