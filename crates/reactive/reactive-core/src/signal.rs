use std::marker::PhantomData;

use crate::runtime::{self, SignalId};

fn read_with<T: 'static, R>(id: SignalId, f: impl FnOnce(&T) -> R) -> R {
    runtime::track_signal(id);
    runtime::with_signal_value::<T, R>(id, f)
}

fn peek_with<T: 'static, R>(id: SignalId, f: impl FnOnce(&T) -> R) -> R {
    runtime::with_signal_value::<T, R>(id, f)
}

pub struct ReadSignal<T: 'static> {
    pub(crate) id: SignalId,
    _marker: PhantomData<T>,
}

impl<T: 'static> Clone for ReadSignal<T> {
    fn clone(&self) -> Self {
        runtime::clone_signal(self.id);
        ReadSignal {
            id: self.id,
            _marker: PhantomData,
        }
    }
}

impl<T: 'static> Drop for ReadSignal<T> {
    fn drop(&mut self) {
        runtime::drop_signal(self.id);
    }
}

impl<T: Clone + 'static> ReadSignal<T> {
    pub fn get(&self) -> T {
        read_with::<T, T>(self.id, |v| v.clone())
    }

    pub fn peek(&self) -> T {
        peek_with::<T, T>(self.id, |v| v.clone())
    }
}

impl<T: 'static> ReadSignal<T> {
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        read_with::<T, R>(self.id, f)
    }
}

pub struct RwSignal<T: 'static> {
    pub(crate) id: SignalId,
    _marker: PhantomData<T>,
}

impl<T: 'static> Clone for RwSignal<T> {
    fn clone(&self) -> Self {
        runtime::clone_signal(self.id);
        RwSignal {
            id: self.id,
            _marker: PhantomData,
        }
    }
}

impl<T: 'static> Drop for RwSignal<T> {
    fn drop(&mut self) {
        runtime::drop_signal(self.id);
    }
}

impl<T: 'static> RwSignal<T> {
    pub fn set(&self, value: T) {
        runtime::set_signal_value(self.id, value);
        runtime::notify_signal(self.id);
    }

    pub fn update(&self, f: impl FnOnce(&mut T)) {
        runtime::update_signal_value::<T>(self.id, f);
        runtime::notify_signal(self.id);
    }

    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        read_with::<T, R>(self.id, f)
    }

    pub fn read_only(&self) -> ReadSignal<T> {
        runtime::clone_signal(self.id);
        ReadSignal {
            id: self.id,
            _marker: PhantomData,
        }
    }
}

impl<T: Clone + 'static> RwSignal<T> {
    pub fn get(&self) -> T {
        read_with::<T, T>(self.id, |v| v.clone())
    }

    pub fn peek(&self) -> T {
        peek_with::<T, T>(self.id, |v| v.clone())
    }
}

pub fn signal<T: 'static>(value: T) -> RwSignal<T> {
    let id = runtime::create_signal_storage(value, 1);
    RwSignal {
        id,
        _marker: PhantomData,
    }
}
