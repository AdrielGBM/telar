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
}

thread_local! {
    static RUNTIME: RefCell<Runtime> = RefCell::new(Runtime {
        observer_stack: Vec::new(),
        effects: Vec::new(),
        batch_depth: 0,
        pending: Vec::new(),
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

pub(crate) fn schedule(id: EffectId) {
    let should_flush = RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        if !rt.pending.contains(&id) {
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

fn flush() {
    loop {
        let pending = RUNTIME.with(|rt| std::mem::take(&mut rt.borrow_mut().pending));
        if pending.is_empty() {
            break;
        }
        for id in pending {
            run_effect(id);
        }
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
