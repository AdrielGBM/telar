use std::cell::RefCell;
use std::rc::Rc;

pub(crate) type EffectId = usize;

pub(crate) struct EffectEntry {
    pub(crate) f: Rc<dyn Fn()>,
}

struct Runtime {
    observer_stack: Vec<EffectId>,
    effects: Vec<Option<EffectEntry>>,
    batch_depth: usize,
    pending: Vec<EffectId>,
    on_flush: Option<Rc<dyn Fn()>>,
}

thread_local! {
    static RUNTIME: RefCell<Runtime> = RefCell::new(Runtime {
        observer_stack: Vec::new(),
        effects: Vec::new(),
        batch_depth: 0,
        pending: Vec::new(),
        on_flush: None,
    });
}

pub fn set_flush_notify(f: impl Fn() + 'static) {
    RUNTIME.with(|rt| {
        rt.borrow_mut().on_flush = Some(Rc::new(f));
    });
}

pub(crate) fn current_observer() -> Option<EffectId> {
    RUNTIME.with(|rt| rt.borrow().observer_stack.last().copied())
}

pub(crate) fn register_effect(f: Rc<dyn Fn()>) -> EffectId {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let id = rt.effects.len();
        rt.effects.push(Some(EffectEntry { f }));
        id
    })
}

pub(crate) fn deregister_effect(id: EffectId) {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        if let Some(slot) = rt.effects.get_mut(id) {
            *slot = None;
        }
    });
}

pub(crate) fn is_alive(id: EffectId) -> bool {
    RUNTIME.with(|rt| {
        rt.borrow()
            .effects
            .get(id)
            .map(|e| e.is_some())
            .unwrap_or(false)
    })
}

pub(crate) fn schedule(id: EffectId) {
    let should_flush = RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let alive = rt.effects.get(id).map(|e| e.is_some()).unwrap_or(false);
        if alive && !rt.pending.contains(&id) {
            rt.pending.push(id);
        }
        rt.batch_depth == 0
    });
    if should_flush {
        flush();
    }
}

pub(crate) fn run_effect(id: EffectId) {
    let f = RUNTIME.with(|rt| {
        rt.borrow()
            .effects
            .get(id)
            .and_then(|e| e.as_ref().map(|e| Rc::clone(&e.f)))
    });
    if let Some(f) = f {
        RUNTIME.with(|rt| rt.borrow_mut().observer_stack.push(id));
        f();
        RUNTIME.with(|rt| rt.borrow_mut().observer_stack.pop());
    }
}

const MAX_FLUSH_ITERATIONS: usize = 1_000;

fn flush() {
    let mut did_work = false;
    for _ in 0..MAX_FLUSH_ITERATIONS {
        let pending = RUNTIME.with(|rt| std::mem::take(&mut rt.borrow_mut().pending));
        if pending.is_empty() {
            if did_work {
                let cb = RUNTIME.with(|rt| rt.borrow().on_flush.as_ref().map(Rc::clone));
                if let Some(cb) = cb {
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
    panic!(
        "reactive flush exceeded {MAX_FLUSH_ITERATIONS} iterations — \
         likely an effect is writing to a signal it depends on"
    );
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
