use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use crate::runtime::{self, EffectId};

struct MemoInner<T> {
    value: Option<T>,
    subscribers: BTreeSet<EffectId>,
}

pub struct Memo<T: 'static> {
    inner: Rc<RefCell<MemoInner<T>>>,
}

impl<T: 'static> Clone for Memo<T> {
    fn clone(&self) -> Self {
        Memo {
            inner: Rc::clone(&self.inner),
        }
    }
}

impl<T: Clone + 'static> Memo<T> {
    pub fn get(&self) -> T {
        self.track();
        self.inner
            .borrow()
            .value
            .as_ref()
            .expect("memo value uninitialized — possible reactive cycle")
            .clone()
    }

    pub fn try_get(&self) -> Option<T> {
        self.track();
        self.inner.borrow().value.as_ref().cloned()
    }
}

impl<T: 'static> Memo<T> {
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        self.track();
        f(self
            .inner
            .borrow()
            .value
            .as_ref()
            .expect("memo value uninitialized — possible reactive cycle"))
    }

    fn track(&self) {
        if let Some(id) = runtime::current_observer() {
            let mut inner = self.inner.borrow_mut();
            inner.subscribers.insert(id);
        }
    }
}

pub fn create_memo<T: 'static>(f: impl Fn() -> T + 'static) -> Memo<T> {
    let inner: Rc<RefCell<MemoInner<T>>> = Rc::new(RefCell::new(MemoInner {
        value: None,
        subscribers: BTreeSet::new(),
    }));
    let inner_clone = Rc::clone(&inner);

    let effect_f: Rc<dyn Fn()> = Rc::new(move || {
        let new_value = f();
        let subs = {
            let mut memo = inner_clone.borrow_mut();
            memo.value = Some(new_value);
            memo.subscribers.iter().copied().collect::<Vec<_>>()
        };
        for id in subs {
            runtime::schedule(id);
        }
    });

    let id = runtime::register_effect(Rc::clone(&effect_f));
    runtime::run_effect(id);

    Memo { inner }
}
