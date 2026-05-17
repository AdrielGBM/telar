use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use crate::runtime::{self, EffectId};

pub(crate) struct SignalInner<T> {
    pub(crate) value: T,
    pub(crate) subscribers: BTreeSet<EffectId>,
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

impl<T: Eq + 'static> WriteSignal<T> {
    pub fn set_if_changed(&self, value: T) {
        if self.inner.borrow().value != value {
            self.inner.borrow_mut().value = value;
            notify(&self.inner);
        }
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

impl<T: Eq + 'static> RwSignal<T> {
    pub fn set_if_changed(&self, value: T) {
        if self.inner.borrow().value != value {
            self.inner.borrow_mut().value = value;
            notify(&self.inner);
        }
    }
}

pub fn create_signal<T: 'static>(value: T) -> (ReadSignal<T>, WriteSignal<T>) {
    let inner = Rc::new(RefCell::new(SignalInner {
        value,
        subscribers: BTreeSet::new(),
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
            subscribers: BTreeSet::new(),
        })),
    }
}

fn track<T>(inner: &Rc<RefCell<SignalInner<T>>>) {
    if let Some(id) = runtime::current_observer() {
        inner.borrow_mut().subscribers.insert(id);
    }
}

pub(crate) fn notify<T>(inner: &Rc<RefCell<SignalInner<T>>>) {
    let subs: Vec<EffectId> = inner.borrow().subscribers.iter().copied().collect();
    let mut dead = Vec::new();
    for id in subs {
        if runtime::is_alive(id) {
            runtime::schedule(id);
        } else {
            dead.push(id);
        }
    }
    if !dead.is_empty() {
        let mut inner = inner.borrow_mut();
        for id in dead {
            inner.subscribers.remove(&id);
        }
    }
}
