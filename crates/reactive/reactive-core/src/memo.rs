use std::cell::RefCell;
use std::rc::Rc;

use smallvec::SmallVec;

use crate::runtime::{self, EffectId};

enum MemoState<T> {
    Computing,
    Ready(T),
}

struct MemoInner<T> {
    state: MemoState<T>,
    subscribers: SmallVec<[EffectId; 4]>,
    effect_id: EffectId,
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

impl<T: 'static> Drop for Memo<T> {
    fn drop(&mut self) {
        if Rc::strong_count(&self.inner) == 1 {
            let id = self.inner.borrow().effect_id;
            runtime::deregister_effect(id);
        }
    }
}

impl<T: Clone + 'static> Memo<T> {
    pub fn get(&self) -> T {
        self.track();
        match &self.inner.borrow().state {
            MemoState::Ready(v) => v.clone(),
            MemoState::Computing => panic!("reactive cycle detected in memo"),
        }
    }

    pub fn try_get(&self) -> Option<T> {
        self.track();
        match &self.inner.borrow().state {
            MemoState::Ready(v) => Some(v.clone()),
            MemoState::Computing => None,
        }
    }
}

impl<T: 'static> Memo<T> {
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        self.track();
        let borrow = self.inner.borrow();
        match &borrow.state {
            MemoState::Ready(v) => f(v),
            MemoState::Computing => panic!("reactive cycle detected in memo"),
        }
    }

    pub fn try_with<R>(&self, f: impl FnOnce(&T) -> R) -> Option<R> {
        self.track();
        let borrow = self.inner.borrow();
        match &borrow.state {
            MemoState::Ready(v) => Some(f(v)),
            MemoState::Computing => None,
        }
    }

    fn track(&self) {
        if let Some(id) = runtime::current_observer() {
            let mut inner = self.inner.borrow_mut();
            if !inner.subscribers.contains(&id) {
                inner.subscribers.push(id);
            }
        }
    }
}

pub fn create_memo<T: 'static>(f: impl Fn() -> T + 'static) -> Memo<T> {
    use std::rc::Weak;

    let inner: Rc<RefCell<MemoInner<T>>> = Rc::new(RefCell::new(MemoInner {
        state: MemoState::Computing,
        subscribers: SmallVec::new(),
        effect_id: 0,
    }));

    let weak: Weak<RefCell<MemoInner<T>>> = Rc::downgrade(&inner);

    let effect_f: Rc<dyn Fn()> = Rc::new(move || {
        let Some(inner) = weak.upgrade() else {
            return;
        };
        inner.borrow_mut().state = MemoState::Computing;
        let new_value = f();
        let subs: Vec<EffectId> = {
            let mut memo = inner.borrow_mut();
            memo.state = MemoState::Ready(new_value);
            memo.subscribers.iter().copied().collect()
        };
        let mut dead = Vec::new();
        for id in subs {
            if runtime::is_alive(id) {
                runtime::schedule(id);
            } else {
                dead.push(id);
            }
        }
        if !dead.is_empty() {
            inner.borrow_mut().subscribers.retain(|x| !dead.contains(x));
        }
    });

    let effect_id = runtime::register_effect(Rc::clone(&effect_f));
    inner.borrow_mut().effect_id = effect_id;

    runtime::run_effect(effect_id);

    Memo { inner }
}
