//! [`RwSignal`] and [`ReadSignal`]: `Copy` handles into the runtime's signal arena.

use std::marker::PhantomData;

use crate::runtime::{self, SignalId};

fn read_with<T: 'static, R>(id: SignalId, f: impl FnOnce(&T) -> R) -> R {
    runtime::track_signal(id);
    runtime::with_signal_value::<T, R>(id, f)
}

fn peek_with<T: 'static, R>(id: SignalId, f: impl FnOnce(&T) -> R) -> R {
    runtime::with_signal_value::<T, R>(id, f)
}

fn try_read_with<T: 'static, R>(id: SignalId, f: impl FnOnce(&T) -> R) -> Option<R> {
    runtime::track_signal(id);
    runtime::try_with_signal_value::<T, R>(id, f)
}

/// A read handle on a signal.
///
/// `Copy`, and the reason is the whole of the owner tree: the handle is an id, the *owner* is what frees the storage, and nothing has to be moved or cloned to be read twice. What that trades away is a compile-time guarantee for a runtime one — a handle outliving its owner used to be impossible to write and is now a checked failure against the version in the key. Leptos and Dioxus each made the same trade, both after trying the other road.
pub struct ReadSignal<T: 'static> {
    pub(crate) id: SignalId,
    _marker: PhantomData<T>,
}

// Hand-written, because `#[derive]` would bound these on `T` and `RwSignal<String>` would not be `Copy`. The parameter names what the signal holds, never what the handle stores.
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

    /// [`get`](Self::get), answering `None` rather than panicking when the storage is gone.
    pub fn try_get(&self) -> Option<T> {
        try_read_with::<T, T>(self.id, |v| v.clone())
    }
}

impl<T: 'static> ReadSignal<T> {
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        read_with::<T, R>(self.id, f)
    }

    /// [`with`](Self::with), answering `None` rather than panicking when the storage is gone.
    pub fn try_with<R>(&self, f: impl FnOnce(&T) -> R) -> Option<R> {
        try_read_with::<T, R>(self.id, f)
    }

    /// Whether the storage this handle names is still there.
    ///
    /// For the handle that legitimately outlives its owner — one kept in a store the tree does not own — so it can ask rather than find out by crashing. A handle that lives inside the tree that built it never needs this: its owner outlives it by construction.
    pub fn is_alive(&self) -> bool {
        runtime::signal_is_alive(self.id)
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

    /// As [`with`](Self::with), but does not subscribe the caller — for reads from an event handler, where tracking would attach the value to whatever effect happens to be running.
    pub fn peek_with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        peek_with::<T, R>(self.id, f)
    }

    /// [`with`](Self::with), answering `None` rather than panicking when the storage is gone.
    pub fn try_with<R>(&self, f: impl FnOnce(&T) -> R) -> Option<R> {
        try_read_with::<T, R>(self.id, f)
    }

    /// Whether the storage this handle names is still there.
    ///
    /// For the handle that legitimately outlives its owner — one kept in a store the tree does not own — so it can ask rather than find out by crashing. A handle that lives inside the tree that built it never needs this: its owner outlives it by construction.
    pub fn is_alive(&self) -> bool {
        runtime::signal_is_alive(self.id)
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

    /// [`get`](Self::get), answering `None` rather than panicking when the storage is gone.
    pub fn try_get(&self) -> Option<T> {
        try_read_with::<T, T>(self.id, |v| v.clone())
    }
}

impl RwSignal<bool> {
    /// Flip a boolean signal in place — sugar for `.update(|v| *v = !*v)`, so `$flag.toggle()` reads cleanly.
    pub fn toggle(&self) {
        self.update(|v| *v = !*v);
    }
}

/// Creates a signal holding `value`, owned by whatever scope is active.
pub fn signal<T: 'static>(value: T) -> RwSignal<T> {
    RwSignal {
        id: runtime::create_signal_storage(value),
        _marker: PhantomData,
    }
}
