use std::cell::RefCell;
use std::rc::Rc;

use smallvec::SmallVec;

use crate::runtime::{self, EffectId};

pub(crate) struct SignalInner<T> {
    pub(crate) value: T,
    pub(crate) subscribers: SmallVec<[EffectId; 4]>,
}
pub struct ReadSignal<T: 'static> {
    pub(crate) inner: Rc<RefCell<SignalInner<T>>>,
}

impl<T: 'static> Clone for ReadSignal<T> {
    fn clone(&self) -> Self {
        ReadSignal {
            inner: Rc::clone(&self.inner),
        }
    }
}

impl<T: Clone + 'static> ReadSignal<T> {
    pub fn get(&self) -> T {
        track(&self.inner);
        self.inner.borrow().value.clone()
    }

    pub fn peek(&self) -> T {
        self.inner.borrow().value.clone()
    }
}

impl<T: 'static> ReadSignal<T> {
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        track(&self.inner);
        let borrow = self.inner.borrow();
        let result = f(&borrow.value);
        drop(borrow);
        result
    }
}

pub struct WriteSignal<T: 'static> {
    pub(crate) inner: Rc<RefCell<SignalInner<T>>>,
}

impl<T: 'static> Clone for WriteSignal<T> {
    fn clone(&self) -> Self {
        WriteSignal {
            inner: Rc::clone(&self.inner),
        }
    }
}

impl<T: 'static> WriteSignal<T> {
    pub fn set(&self, value: T) {
        self.inner.borrow_mut().value = value;
        notify(&self.inner);
    }

    pub fn update(&self, f: impl FnOnce(&mut T)) {
        let mut borrow = self.inner.borrow_mut();
        f(&mut borrow.value);
        drop(borrow);
        notify(&self.inner);
    }
}

pub struct RwSignal<T: 'static> {
    inner: Rc<RefCell<SignalInner<T>>>,
}

impl<T: 'static> Clone for RwSignal<T> {
    fn clone(&self) -> Self {
        RwSignal {
            inner: Rc::clone(&self.inner),
        }
    }
}

impl<T: 'static> RwSignal<T> {
    pub fn set(&self, value: T) {
        self.inner.borrow_mut().value = value;
        notify(&self.inner);
    }

    pub fn update(&self, f: impl FnOnce(&mut T)) {
        let mut borrow = self.inner.borrow_mut();
        f(&mut borrow.value);
        drop(borrow);
        notify(&self.inner);
    }

    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        track(&self.inner);
        let borrow = self.inner.borrow();
        let result = f(&borrow.value);
        drop(borrow);
        result
    }

    pub fn read_only(&self) -> ReadSignal<T> {
        ReadSignal {
            inner: Rc::clone(&self.inner),
        }
    }

    pub fn write_only(&self) -> WriteSignal<T> {
        WriteSignal {
            inner: Rc::clone(&self.inner),
        }
    }
}

impl<T: Clone + 'static> RwSignal<T> {
    pub fn get(&self) -> T {
        track(&self.inner);
        self.inner.borrow().value.clone()
    }

    pub fn peek(&self) -> T {
        self.inner.borrow().value.clone()
    }
}

pub fn create_signal<T: 'static>(value: T) -> (ReadSignal<T>, WriteSignal<T>) {
    let inner = Rc::new(RefCell::new(SignalInner {
        value,
        subscribers: SmallVec::new(),
    }));
    (
        ReadSignal {
            inner: Rc::clone(&inner),
        },
        WriteSignal { inner },
    )
}

pub fn create_rw_signal<T: 'static>(value: T) -> RwSignal<T> {
    RwSignal {
        inner: Rc::new(RefCell::new(SignalInner {
            value,
            subscribers: SmallVec::new(),
        })),
    }
}

fn track<T>(inner: &Rc<RefCell<SignalInner<T>>>) {
    if let Some(id) = runtime::current_observer() {
        let mut borrow = inner.borrow_mut();
        if !borrow.subscribers.contains(&id) {
            borrow.subscribers.push(id);
        }
    }
}

pub(crate) fn notify<T>(inner: &Rc<RefCell<SignalInner<T>>>) {
    let subs: SmallVec<[EffectId; 8]> = inner.borrow().subscribers.iter().copied().collect();
    if subs.is_empty() {
        return;
    }
    // Single RUNTIME.with call to schedule all subscribers at once
    let (should_flush, dead): (bool, SmallVec<[EffectId; 4]>) = runtime::with_runtime_mut(|rt| {
        let mut dead: SmallVec<[EffectId; 4]> = SmallVec::new();
        let mut any_scheduled = false;
        for &id in &subs {
            if rt.effects.contains(id) {
                if rt.pending_set.insert(id) {
                    if rt.effects[id].pure {
                        rt.memo_pending.push(id);
                    } else {
                        rt.pending.push(id);
                    }
                }
                any_scheduled = true;
            } else {
                dead.push(id);
            }
        }
        (any_scheduled && rt.batch_depth == 0 && !rt.flushing, dead)
    });
    // Clean up dead subscribers outside of RUNTIME borrow
    if !dead.is_empty() {
        let mut borrow = inner.borrow_mut();
        borrow.subscribers.retain(|x| !dead.contains(x));
    }
    if should_flush {
        runtime::flush_public();
    }
}
