use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

pub(crate) type EffectId = usize;

pub(crate) struct EffectEntry {
    pub(crate) f: Rc<dyn Fn()>,
}

struct Runtime {
    observer_stack: Vec<EffectId>,
    effects: slab::Slab<EffectEntry>,
    batch_depth: usize,
    pending: Vec<EffectId>,
    pending_set: HashSet<EffectId>,
    on_flush: Vec<(u64, Rc<dyn Fn()>)>,
    next_flush_notify_id: u64,
    flushing: bool,
}

thread_local! {
    static RUNTIME: RefCell<Runtime> = RefCell::new(Runtime {
        observer_stack: Vec::new(),
        effects: slab::Slab::new(),
        batch_depth: 0,
        pending: Vec::new(),
        pending_set: HashSet::new(),
        on_flush: Vec::new(),
        next_flush_notify_id: 0,
        flushing: false,
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

pub(crate) fn register_effect(f: Rc<dyn Fn()>) -> EffectId {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        rt.effects.insert(EffectEntry { f })
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
            rt.pending.push(id);
        }
        rt.batch_depth == 0 && !rt.flushing
    });
    if should_flush {
        flush();
    }
}

pub(crate) fn run_effect(id: EffectId) {
    if !RUNTIME.with(|rt| rt.borrow().effects.contains(id)) {
        return;
    }
    let f = RUNTIME.with(|rt| rt.borrow().effects.get(id).map(|e| Rc::clone(&e.f)));
    if let Some(f) = f {
        RUNTIME.with(|rt| rt.borrow_mut().observer_stack.push(id));
        f();
        RUNTIME.with(|rt| rt.borrow_mut().observer_stack.pop());
    }
}

const MAX_FLUSH_ITERATIONS: usize = 1_000;

fn flush() {
    RUNTIME.with(|rt| rt.borrow_mut().flushing = true);

    let should_panic = {
        let mut did_work = false;
        for _ in 0..MAX_FLUSH_ITERATIONS {
            let pending = RUNTIME.with(|rt| {
                let mut rt = rt.borrow_mut();
                rt.pending_set.clear();
                std::mem::take(&mut rt.pending)
            });
            if pending.is_empty() {
                RUNTIME.with(|rt| rt.borrow_mut().flushing = false);
                if did_work {
                    let cbs: Vec<Rc<dyn Fn()>> = RUNTIME.with(|rt| {
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
            for id in pending {
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
        rt.batch_depth == 0 && !rt.pending.is_empty()
    });
    if should_flush {
        flush();
    }
    result
}
