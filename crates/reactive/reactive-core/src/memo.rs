use std::cell::RefCell;
use std::rc::Rc;

use smallvec::SmallVec;

use crate::runtime::{self, EffectId};

enum MemoState<T> {
    Computing, // reading while Computing means the closure re-entered itself: a cycle
    Clean(T),
    Dirty,
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
            MemoState::Clean(v) => v.clone(),
            MemoState::Dirty => panic!("memo read while Dirty — flush ordering issue"),
            MemoState::Computing => panic!("reactive cycle detected in memo"),
        }
    }
}

impl<T: 'static> Memo<T> {
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        self.track();
        let borrow = self.inner.borrow();
        match &borrow.state {
            MemoState::Clean(v) => f(v),
            MemoState::Dirty => panic!("memo read while Dirty — flush ordering issue"),
            MemoState::Computing => panic!("reactive cycle detected in memo"),
        }
    }

    fn track(&self) {
        if let Some(id) = runtime::current_observer() {
            let mut borrow = self.inner.borrow_mut();
            if !borrow.subscribers.contains(&id) {
                borrow.subscribers.push(id);
            }
        }
    }
}

pub fn memo<T: PartialEq + 'static>(f: impl Fn() -> T + 'static) -> Memo<T> {
    use std::rc::Weak;

    let inner: Rc<RefCell<MemoInner<T>>> = Rc::new(RefCell::new(MemoInner {
        state: MemoState::Dirty,
        subscribers: SmallVec::new(),
        effect_id: 0,
    }));

    let weak: Weak<RefCell<MemoInner<T>>> = Rc::downgrade(&inner);

    let effect_f: Box<dyn Fn()> = Box::new(move || {
        let Some(inner) = weak.upgrade() else {
            return;
        };
        inner.borrow_mut().state = MemoState::Computing;
        let new_value = f();
        let subs: SmallVec<[EffectId; 8]> = {
            let mut memo = inner.borrow_mut();
            let changed = match &memo.state {
                MemoState::Clean(old) => old != &new_value,
                _ => true,
            };
            memo.state = MemoState::Clean(new_value);
            if changed {
                memo.subscribers.iter().copied().collect()
            } else {
                SmallVec::new()
            }
        };
        let mut dead: Option<Vec<EffectId>> = None;
        for id in subs {
            if runtime::is_alive(id) {
                runtime::schedule(id);
            } else {
                dead.get_or_insert_with(Vec::new).push(id);
            }
        }
        if let Some(dead) = dead {
            let mut memo = inner.borrow_mut();
            for id in dead {
                memo.subscribers.retain(|x| x != &id);
            }
        }
    });

    // Register as a pure effect so it runs before user effects during flush
    let effect_id = runtime::register_pure_effect(effect_f);
    inner.borrow_mut().effect_id = effect_id;

    runtime::run_effect(effect_id);

    Memo { inner }
}
