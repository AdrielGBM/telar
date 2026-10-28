//! [`Memo`]: a derived value recomputed on demand and only notified onward when it actually changed.

use std::cell::Cell;
use std::marker::PhantomData;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::rc::Rc;

use crate::runtime::{self, EffectId, HeldAt, SignalId, SiteCell};

enum MemoState<T> {
    /// Reading it means the computation re-entered itself: a cycle.
    Computing,
    Clean(T),
    /// No value: before the first computation, and after one that panicked.
    Dirty,
}

struct MemoNode<T> {
    state: SiteCell<MemoState<T>>,
    /// A recompute found the value lent to a read and could not run, so the value is stale until the next one does.
    missed: Cell<bool>,
    recompute: Cell<Option<EffectId>>,
}

impl<T: 'static> MemoNode<T> {
    /// Recomputes a memo that has no current value, unless the value is still lent to an outer read, which recomputes it once it returns.
    fn catch_up(&self) {
        let Some(recompute) = self.recompute.get() else {
            return;
        };
        let behind = if self.missed.get() {
            self.state.is_free()
        } else {
            self.state
                .try_read()
                .is_ok_and(|state| matches!(*state, MemoState::Dirty))
        };
        if behind {
            runtime::force_run_effect(recompute);
        }
    }

    fn set_state(&self, state: MemoState<T>) {
        match self.state.try_write() {
            Ok(mut slot) => *slot = state,
            Err(held) => recomputed_while_read::<T>(&held),
        }
    }
}

/// Queues the recompute a read made miss once that read lets go of the value.
struct RecomputeMissed<'a, T>(&'a MemoNode<T>);

impl<T> Drop for RecomputeMissed<'_, T> {
    fn drop(&mut self) {
        let node = self.0;
        if node.missed.get()
            && node.state.is_free()
            && let Some(recompute) = node.recompute.get()
        {
            runtime::reschedule_memo(recompute, !std::thread::panicking());
        }
    }
}

#[cold]
fn recomputed_while_read<T>(held: &HeldAt) -> ! {
    let type_name = std::any::type_name::<T>();
    panic!(
        "memo of type {type_name} recomputed inside one of its own reads.\n  \
         read at: {held}\n\
         A memo's sources cannot change and flush while its value is lent out — a `Clone` or a `.with` closure that \
         writes them, say. It keeps the value that read is using and recomputes once the read returns."
    )
}

/// A derived value, recomputed when what it reads moves; `Copy` and `!Send` like the signal handles, since the handle is an id into this thread's arena and the owner is what disposes it.
pub struct Memo<T: 'static> {
    id: SignalId,
    _marker: PhantomData<T>,
    _ui_thread_only: PhantomData<*const ()>,
}

impl<T: 'static> Clone for Memo<T> {
    fn clone(&self) -> Self {
        *self
    }
}

// Hand-written rather than derived: `#[derive(Copy)]` would demand `T: Copy`, and the parameter only names what the memo computes, never what the handle stores.
impl<T: 'static> Copy for Memo<T> {}

impl<T: 'static> Memo<T> {
    fn node(&self) -> Rc<MemoNode<T>> {
        runtime::shared_storage(self.id)
    }

    /// Lends the value to `f`, which may read anything and write any signal, this memo's own sources included, and the effects those writes wake run after it returns.
    #[track_caller]
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        let batch = runtime::OpenBatch::open();
        let result = self.read(f);
        batch.close();
        result
    }

    /// Whether the storage behind this handle is still there — see [`crate::RwSignal::is_alive`].
    pub fn is_alive(&self) -> bool {
        runtime::signal_is_alive(self.id)
    }

    #[track_caller]
    fn read<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        let node = self.node();
        runtime::track_signal(self.id);
        node.catch_up();
        let _recompute_missed = RecomputeMissed(&node);
        let state = match node.state.try_read() {
            Ok(state) => state,
            Err(held) => recomputed_while_read::<T>(&held),
        };
        match &*state {
            MemoState::Clean(value) => f(value),
            MemoState::Dirty => panic!(
                "memo of type {} has no value: its computation panicked",
                std::any::type_name::<T>()
            ),
            MemoState::Computing => panic!(
                "reactive cycle detected in memo of type {}",
                std::any::type_name::<T>()
            ),
        }
    }
}

impl<T: Clone + 'static> Memo<T> {
    /// The value, cloned while it is lent, so a `Clone` that writes this memo's sources and flushes panics naming it rather than reading a value mid-recompute.
    #[track_caller]
    pub fn get(&self) -> T {
        self.read(T::clone)
    }

    /// [`get`](Self::get), answering `None` rather than panicking when the storage is gone.
    #[track_caller]
    pub fn try_get(&self) -> Option<T> {
        if self.is_alive() {
            Some(self.get())
        } else {
            None
        }
    }
}

/// A derived value recomputed when its sources change, notifying onward only when the result differs.
pub fn memo<T: PartialEq + 'static>(f: impl Fn() -> T + 'static) -> Memo<T> {
    let node = Rc::new(MemoNode {
        state: SiteCell::new(MemoState::Dirty),
        missed: Cell::new(false),
        recompute: Cell::new(None),
    });
    // Stored as a signal, so the owner disposes it and its readers subscribe and compare versions as they do with a signal.
    let id = runtime::create_shared_storage(Rc::clone(&node));

    // The node is reached through a `Weak` because the recompute runs during a flush that may come after disposal, which a `Weak` that fails to upgrade reports.
    let weak = Rc::downgrade(&node);
    let effect_f: Box<dyn Fn()> = Box::new(move || {
        let Some(node) = weak.upgrade() else {
            return;
        };
        // Held aside rather than overwritten: it is what the new value is compared against.
        let previous = match node.state.try_write() {
            Ok(mut state) => std::mem::replace(&mut *state, MemoState::Computing),
            Err(held) => {
                node.missed.set(true);
                recomputed_while_read::<T>(&held)
            }
        };
        node.missed.set(false);
        let new_value = match catch_unwind(AssertUnwindSafe(&f)) {
            Ok(value) => value,
            Err(payload) => {
                node.set_state(MemoState::Dirty);
                resume_unwind(payload);
            }
        };
        let changed = match &previous {
            MemoState::Clean(old) => old != &new_value,
            _ => true,
        };
        node.set_state(MemoState::Clean(new_value));
        if changed {
            runtime::notify_signal(id);
        }
    });

    // The owner active here recorded it, and that is what disposes it.
    let effect_id = runtime::register_memo_effect(effect_f, id);
    node.recompute.set(Some(effect_id));
    drop(node);
    runtime::run_effect(effect_id);

    Memo {
        id,
        _marker: PhantomData,
        _ui_thread_only: PhantomData,
    }
}

#[cfg(test)]
#[path = "memo_test.rs"]
mod tests;
