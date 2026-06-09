use std::cell::RefCell;
use std::rc::Rc;

use rustc_hash::FxHashSet;

pub(crate) type EffectId = usize;

pub(crate) struct EffectEntry {
    pub(crate) f: Box<dyn Fn()>,
    pub(crate) pure: bool,
    pub(crate) last_run_epoch: u64,
}

pub(crate) struct Runtime {
    pub(crate) observer_stack: Vec<EffectId>,
    pub(crate) effects: slab::Slab<EffectEntry>,
    pub(crate) batch_depth: usize,
    pub(crate) pending: Vec<EffectId>,
    pub(crate) memo_pending: Vec<EffectId>,
    pub(crate) pending_set: FxHashSet<EffectId>,
    on_flush: Vec<(u64, Rc<dyn Fn()>)>,
    next_flush_notify_id: u64,
    pub(crate) flushing: bool,
    flush_epoch: u64,
}

thread_local! {
    static RUNTIME: RefCell<Runtime> = RefCell::new(Runtime {
        observer_stack: Vec::new(),
        effects: slab::Slab::new(),
        batch_depth: 0,
        pending: Vec::new(),
        memo_pending: Vec::new(),
        pending_set: FxHashSet::default(),
        on_flush: Vec::new(),
        next_flush_notify_id: 0,
        flushing: false,
        flush_epoch: 0,
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
        rt.effects.insert(EffectEntry {
            f,
            pure: false,
            last_run_epoch: 0,
        })
    })
}

pub(crate) fn register_pure_effect(f: Box<dyn Fn()>) -> EffectId {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        rt.effects.insert(EffectEntry {
            f,
            pure: true,
            last_run_epoch: 0,
        })
    })
}

pub(crate) fn deregister_effect(id: EffectId) {
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

pub(crate) fn schedule(id: EffectId) {
    let should_flush = RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let alive = rt.effects.contains(id);
        if alive && rt.pending_set.insert(id) {
            if rt.effects[id].pure {
                rt.memo_pending.push(id);
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

pub(crate) fn with_runtime_mut<R>(f: impl FnOnce(&mut Runtime) -> R) -> R {
    RUNTIME.with(|rt| f(&mut rt.borrow_mut()))
}

pub(crate) fn run_effect(id: EffectId) {
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
        // SAFETY: The slab owns this Box. No effect closure in this codebase captures its own Effect handle, so deregistration cannot happen during execution.
        let ptr: *const dyn Fn() = &*rt.effects[id].f;
        rt.observer_stack.push(id);
        Some(ptr)
    });
    if let Some(ptr) = ptr {
        struct PopGuard;
        impl Drop for PopGuard {
            fn drop(&mut self) {
                RUNTIME.with(|rt| rt.borrow_mut().observer_stack.pop());
            }
        }
        let _guard = PopGuard;
        unsafe { (*ptr)() };
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
            // Drain memo_pending first (pure computations), then user effects
            let memo_batch = RUNTIME.with(|rt| {
                let mut rt = rt.borrow_mut();
                std::mem::take(&mut rt.memo_pending)
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
