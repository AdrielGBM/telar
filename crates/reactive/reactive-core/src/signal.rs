use std::marker::PhantomData;

use crate::runtime::{self, SignalId};

fn read_with<T: 'static, R>(id: SignalId, f: impl FnOnce(&T) -> R) -> R {
    runtime::track_signal(id);
    runtime::with_signal_value::<T, R>(id, f)
}

fn peek_with<T: 'static, R>(id: SignalId, f: impl FnOnce(&T) -> R) -> R {
    runtime::with_signal_value::<T, R>(id, f)
}

/// A read handle on a signal.
///
/// `Copy`, and the reason is the whole of [`crate::runtime::owner`]: the handle is an id, the *owner* is what
/// frees the storage, and nothing has to be moved or cloned to be read twice. What that trades away is a
/// compile-time guarantee for a runtime one — a handle outliving its owner used to be impossible to write and
/// is now a checked failure against the version in the key. Leptos and Dioxus each made the same trade, both
/// after trying the other road.
pub struct ReadSignal<T: 'static> {
    pub(crate) id: SignalId,
    _marker: PhantomData<T>,
}

// Hand-written, because `#[derive]` would bound these on `T` — `RwSignal<String>` would not be `Copy`, which is most of the point. The parameter names what the signal holds, never what the handle stores.
impl<T: 'static> Clone for ReadSignal<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> Copy for ReadSignal<T> {}

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

/// A read-write handle on a signal. `Copy`, for the reasons in [`ReadSignal`].
pub struct RwSignal<T: 'static> {
    pub(crate) id: SignalId,
    _marker: PhantomData<T>,
}

impl<T: 'static> Clone for RwSignal<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> Copy for RwSignal<T> {}

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

    /// As [`with`](Self::with), but does not subscribe the caller — for reads from an event handler, where
    /// tracking would attach the value to whatever effect happens to be running.
    pub fn peek_with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        peek_with::<T, R>(self.id, f)
    }

    pub fn read_only(&self) -> ReadSignal<T> {
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

impl RwSignal<bool> {
    /// Flip a boolean signal in place — sugar for `.update(|v| *v = !*v)`, so `$flag.toggle()` reads cleanly.
    pub fn toggle(&self) {
        self.update(|v| *v = !*v);
    }
}

pub fn signal<T: 'static>(value: T) -> RwSignal<T> {
    RwSignal {
        id: runtime::create_signal_storage(value),
        _marker: PhantomData,
    }
}
