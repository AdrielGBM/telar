use std::cell::RefCell;
use std::rc::Rc;

use rustc_hash::FxHashSet;
use smallvec::SmallVec;

use crate::runtime::{self, EffectId};

pub(crate) struct SignalInner<T> {
    pub(crate) value: T,
    pub(crate) subscribers: FxHashSet<EffectId>,
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
        subscribers: FxHashSet::default(),
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
            subscribers: FxHashSet::default(),
        })),
    }
}

fn track<T>(inner: &Rc<RefCell<SignalInner<T>>>) {
    if let Some(id) = runtime::current_observer() {
        inner.borrow_mut().subscribers.insert(id);
    }
}

pub(crate) fn notify<T>(inner: &Rc<RefCell<SignalInner<T>>>) {
    let subs: SmallVec<[EffectId; 8]> = inner.borrow().subscribers.iter().copied().collect();
    let mut dead: Option<Vec<EffectId>> = None;
    for id in subs {
        if runtime::is_alive(id) {
            runtime::schedule(id);
        } else {
            dead.get_or_insert_with(Vec::new).push(id);
        }
    }
    if let Some(dead) = dead {
        let mut inner = inner.borrow_mut();
        for id in dead {
            inner.subscribers.remove(&id);
        }
    }
}
