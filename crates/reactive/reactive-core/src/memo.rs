use std::cell::RefCell;
use std::rc::Rc;

use rustc_hash::FxHashSet;
use smallvec::SmallVec;

use crate::runtime::{self, EffectId};

enum MemoState<T> {
    Computing,
    Ready(T),
}

struct MemoInner<T> {
    state: MemoState<T>,
    subscribers: FxHashSet<EffectId>,
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

    fn track(&self) {
        if let Some(id) = runtime::current_observer() {
            self.inner.borrow_mut().subscribers.insert(id);
        }
    }
}

pub fn create_memo<T: PartialEq + 'static>(f: impl Fn() -> T + 'static) -> Memo<T> {
    use std::rc::Weak;

    let inner: Rc<RefCell<MemoInner<T>>> = Rc::new(RefCell::new(MemoInner {
        state: MemoState::Computing,
        subscribers: FxHashSet::default(),
        effect_id: 0,
    }));

    let weak: Weak<RefCell<MemoInner<T>>> = Rc::downgrade(&inner);

    let effect_f: Rc<dyn Fn()> = Rc::new(move || {
        let Some(inner) = weak.upgrade() else {
            return;
        };
        inner.borrow_mut().state = MemoState::Computing;
        let new_value = f();
        let subs: SmallVec<[EffectId; 8]> = {
            let mut memo = inner.borrow_mut();
            let changed = match &memo.state {
                MemoState::Ready(old) => old != &new_value,
                _ => true,
            };
            memo.state = MemoState::Ready(new_value);
            if changed {
                memo.subscribers.iter().copied().collect()
            } else {
                SmallVec::new()
            }
        };
        let mut dead: FxHashSet<EffectId> = FxHashSet::default();
        for id in subs {
            if runtime::is_alive(id) {
                runtime::schedule(id);
            } else {
                dead.insert(id);
            }
        }
        if !dead.is_empty() {
            let mut memo = inner.borrow_mut();
            dead.iter().for_each(|id| {
                memo.subscribers.remove(id);
            });
        }
    });

    let effect_id = runtime::register_effect(Rc::clone(&effect_f));
    inner.borrow_mut().effect_id = effect_id;

    runtime::run_effect(effect_id);

    Memo { inner }
}
