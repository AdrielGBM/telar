//! [`RwSignal`] and [`ReadSignal`]: `Copy` handles into the runtime's signal arena.

use std::marker::PhantomData;

use crate::runtime::{self, SignalId};

#[track_caller]
fn read_with<T: 'static, R>(id: SignalId, f: impl FnOnce(&T) -> R) -> R {
    runtime::track_signal(id);
    runtime::with_signal_value::<T, R>(id, f)
}

#[track_caller]
fn peek_with<T: 'static, R>(id: SignalId, f: impl FnOnce(&T) -> R) -> R {
    runtime::with_signal_value::<T, R>(id, f)
}

#[track_caller]
fn try_read_with<T: 'static, R>(id: SignalId, f: impl FnOnce(&T) -> R) -> Option<R> {
    runtime::track_signal(id);
    runtime::try_with_signal_value::<T, R>(id, f)
}

/// Closes `batch` once the read that produced `result` has released the value; arguments evaluate in order, so `lent(OpenBatch::open(), read(..))` opens the batch before the read and keeps the caller's `#[track_caller]` location, which a closure would lose.
fn lent<R>(batch: runtime::OpenBatch, result: R) -> R {
    batch.close();
    result
}

/// A read handle on a signal: `Copy` because the owner, not the handle, frees the storage, so a handle outliving it is a checked failure against its key's version, and `!Send` because the id names a slot only in the creating thread's runtime; a result computed elsewhere arrives through [`spawn_task`](crate::spawn_task)'s `on_done`.
pub struct ReadSignal<T: 'static> {
    pub(crate) id: SignalId,
    _marker: PhantomData<T>,
    _ui_thread_only: PhantomData<*const ()>,
}

// Hand-written, because `#[derive]` would bound these on `T` and `RwSignal<String>` would not be `Copy`. The parameter names what the signal holds, never what the handle stores.
impl<T: 'static> Clone for ReadSignal<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> Copy for ReadSignal<T> {}

impl<T: Clone + 'static> ReadSignal<T> {
    #[track_caller]
    pub fn get(&self) -> T {
        read_with::<T, T>(self.id, |v| v.clone())
    }

    /// The value as it stands, **without subscribing the caller to the next one**.
    ///
    /// The read to reach for inside an effect that goes on to write this signal, or anything feeding it. A write bumps the version whether or not the value changed, so an effect that tracked it here schedules itself the moment it writes, and again on every pass after — not a slow loop but an endless one. A debug build says so the first time it happens.
    ///
    /// Also the read for an event handler, where tracking would attach the value to whichever effect happens to be running.
    #[track_caller]
    pub fn peek(&self) -> T {
        peek_with::<T, T>(self.id, |v| v.clone())
    }

    /// [`get`](Self::get), answering `None` rather than panicking when the storage is gone.
    #[track_caller]
    pub fn try_get(&self) -> Option<T> {
        try_read_with::<T, T>(self.id, |v| v.clone())
    }
}

impl<T: 'static> ReadSignal<T> {
    /// Lends the value to `f`, which may read anything and write any other signal, and the effects those writes wake run after it returns; writing this one inside it panics.
    #[track_caller]
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        lent(runtime::OpenBatch::open(), read_with::<T, R>(self.id, f))
    }

    /// [`with`](Self::with), answering `None` rather than panicking when the storage is gone.
    #[track_caller]
    pub fn try_with<R>(&self, f: impl FnOnce(&T) -> R) -> Option<R> {
        lent(
            runtime::OpenBatch::open(),
            try_read_with::<T, R>(self.id, f),
        )
    }

    /// Whether the storage this handle names is still there.
    ///
    /// For the handle that legitimately outlives its owner — one kept in a store the tree does not own — so it can ask rather than find out by crashing. A handle that lives inside the tree that built it never needs this: its owner outlives it by construction.
    pub fn is_alive(&self) -> bool {
        runtime::signal_is_alive(self.id)
    }

    /// Whether an effect, memo or view segment is subscribed to this signal right now.
    ///
    /// For a producer that only has work to do while somebody is watching, such as a clock that should not run for nobody. It reflects the subscriptions as they stand: a reader that stopped reading on its last run, or was disposed, no longer counts.
    pub fn has_subscribers(&self) -> bool {
        runtime::signal_has_subscribers(self.id)
    }
}

/// A read-write handle on a signal. `Copy` and `!Send`, for the reasons in [`ReadSignal`].
pub struct RwSignal<T: 'static> {
    pub(crate) id: SignalId,
    _marker: PhantomData<T>,
    _ui_thread_only: PhantomData<*const ()>,
}

impl<T: 'static> Clone for RwSignal<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> Copy for RwSignal<T> {}

impl<T: 'static> RwSignal<T> {
    #[track_caller]
    pub fn set(&self, value: T) {
        runtime::set_signal_value(self.id, value);
        runtime::notify_signal(self.id);
    }

    /// Changes the value in place and notifies once; `f` may read and write any other signal, whose effects run after it returns, but touching this one inside it panics.
    #[track_caller]
    pub fn update(&self, f: impl FnOnce(&mut T)) {
        let batch = runtime::OpenBatch::open();
        runtime::update_signal_value::<T>(self.id, f);
        runtime::notify_signal(self.id);
        batch.close();
    }

    /// Lends the value to `f`, which may read anything and write any other signal, and the effects those writes wake run after it returns; writing this one inside it panics.
    #[track_caller]
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        lent(runtime::OpenBatch::open(), read_with::<T, R>(self.id, f))
    }

    /// [`with`](Self::with) without subscribing the caller — see [`peek`](RwSignal::peek) for the read it is, and the loop it is there to keep you out of.
    #[track_caller]
    pub fn peek_with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        lent(runtime::OpenBatch::open(), peek_with::<T, R>(self.id, f))
    }

    /// [`with`](Self::with), answering `None` rather than panicking when the storage is gone.
    #[track_caller]
    pub fn try_with<R>(&self, f: impl FnOnce(&T) -> R) -> Option<R> {
        lent(
            runtime::OpenBatch::open(),
            try_read_with::<T, R>(self.id, f),
        )
    }

    /// Whether the storage this handle names is still there.
    ///
    /// For the handle that legitimately outlives its owner — one kept in a store the tree does not own — so it can ask rather than find out by crashing. A handle that lives inside the tree that built it never needs this: its owner outlives it by construction.
    pub fn is_alive(&self) -> bool {
        runtime::signal_is_alive(self.id)
    }

    /// Whether an effect, memo or view segment is subscribed to this signal right now.
    ///
    /// For a producer that only has work to do while somebody is watching, such as a clock that should not run for nobody. It reflects the subscriptions as they stand: a reader that stopped reading on its last run, or was disposed, no longer counts.
    pub fn has_subscribers(&self) -> bool {
        runtime::signal_has_subscribers(self.id)
    }

    pub fn read_only(&self) -> ReadSignal<T> {
        ReadSignal {
            id: self.id,
            _marker: PhantomData,
            _ui_thread_only: PhantomData,
        }
    }
}

impl<T: Clone + 'static> RwSignal<T> {
    #[track_caller]
    pub fn get(&self) -> T {
        read_with::<T, T>(self.id, |v| v.clone())
    }

    /// The value as it stands, **without subscribing the caller to the next one**.
    ///
    /// The read to reach for inside an effect that goes on to write this signal, or anything feeding it. A write bumps the version whether or not the value changed, so an effect that tracked it here schedules itself the moment it writes, and again on every pass after — not a slow loop but an endless one. A debug build says so the first time it happens.
    ///
    /// Also the read for an event handler, where tracking would attach the value to whichever effect happens to be running.
    #[track_caller]
    pub fn peek(&self) -> T {
        peek_with::<T, T>(self.id, |v| v.clone())
    }

    /// [`get`](Self::get), answering `None` rather than panicking when the storage is gone.
    #[track_caller]
    pub fn try_get(&self) -> Option<T> {
        try_read_with::<T, T>(self.id, |v| v.clone())
    }
}

impl<T: PartialEq + 'static> RwSignal<T> {
    /// Writes `value` only when it differs from the current one, returning whether it wrote. Subscribers are not notified for an equal value, and the comparison does not subscribe the caller.
    #[track_caller]
    pub fn set_if_changed(&self, value: T) -> bool {
        let changed = peek_with::<T, bool>(self.id, |current| *current != value);
        if changed {
            self.set(value);
        }
        changed
    }
}

impl RwSignal<bool> {
    /// Flip a boolean signal in place — sugar for `.update(|v| *v = !*v)`, so `$flag.toggle()` reads cleanly.
    #[track_caller]
    pub fn toggle(&self) {
        self.update(|v| *v = !*v);
    }
}

/// Creates a signal holding `value`, owned by whatever scope is active.
pub fn signal<T: 'static>(value: T) -> RwSignal<T> {
    RwSignal {
        id: runtime::create_signal_storage(value),
        _marker: PhantomData,
        _ui_thread_only: PhantomData,
    }
}
