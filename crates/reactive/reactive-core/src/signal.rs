use std::cell::RefCell;
use std::rc::Rc;

use crate::runtime::{self, EffectId};

pub(crate) struct SignalInner<T> {
    pub(crate) value: T,
    pub(crate) subscribers: Vec<EffectId>,
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
}

impl<T: 'static> ReadSignal<T> {
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        track(&self.inner);
        f(&self.inner.borrow().value)
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
        f(&mut self.inner.borrow_mut().value);
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
        f(&mut self.inner.borrow_mut().value);
        notify(&self.inner);
    }

    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        track(&self.inner);
        f(&self.inner.borrow().value)
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
}

pub fn create_signal<T: 'static>(value: T) -> (ReadSignal<T>, WriteSignal<T>) {
    let inner = Rc::new(RefCell::new(SignalInner {
        value,
        subscribers: Vec::new(),
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
            subscribers: Vec::new(),
        })),
    }
}

fn track<T>(inner: &Rc<RefCell<SignalInner<T>>>) {
    if let Some(id) = runtime::current_observer() {
        let mut inner = inner.borrow_mut();
        if !inner.subscribers.contains(&id) {
            inner.subscribers.push(id);
        }
    }
}

pub(crate) fn notify<T>(inner: &Rc<RefCell<SignalInner<T>>>) {
    let subs = inner.borrow().subscribers.clone();
    for id in subs {
        runtime::schedule(id);
    }
}
