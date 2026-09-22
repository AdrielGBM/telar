//! [`Memo`]: a derived value recomputed on demand and only notified onward when it actually changed.

use std::cell::RefCell;
use std::marker::PhantomData;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::rc::Rc;

use smallvec::SmallVec;

use crate::runtime::{self, EffectId, SignalId};

enum MemoState<T> {
    Computing, // reading while Computing means the closure re-entered itself: a cycle
    Clean(T),
    /// No value: before the first computation, and after one that panicked.
    Dirty,
}

struct MemoInner<T> {
    state: MemoState<T>,
    subscribers: SmallVec<[EffectId; 4]>,
    recompute: Option<EffectId>,
}

/// A derived value, recomputed when what it reads moves.
///
/// `Copy`, like the signal handles, and for the same reason: the handle is an id into the runtime's arena and the *owner* is what disposes it. It used to be `Rc`-backed and kept alive by whoever read it, which is why it was the one handle a `.rsx` view still had to clone by hand.
///
/// The `Rc<RefCell<…>>` did not disappear so much as move: it is the *value* the arena slot holds, reached through the id, so the memo's own state still has one owner while the handle has none.
pub struct Memo<T: 'static> {
    id: SignalId,
    _marker: PhantomData<T>,
}

impl<T: 'static> Clone for Memo<T> {
    fn clone(&self) -> Self {
        *self
    }
}

// Hand-written rather than derived: `#[derive(Copy)]` would demand `T: Copy`, and the parameter only names what the memo computes, never what the handle stores.
impl<T: 'static> Copy for Memo<T> {}

type Shared<T> = Rc<RefCell<MemoInner<T>>>;

impl<T: 'static> Memo<T> {
    fn shared(&self) -> Shared<T> {
        runtime::with_signal_value::<Shared<T>, _>(self.id, Rc::clone)
    }

    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        let inner = self.shared();
        self.track(&inner);
        // A computation that panicked left nothing to read, and nothing reschedules it until a source moves: the read computes it again, so it either answers or panics with the real cause.
        let recompute = {
            let borrow = inner.borrow();
            matches!(borrow.state, MemoState::Dirty)
                .then_some(borrow.recompute)
                .flatten()
        };
        if let Some(recompute) = recompute {
            runtime::force_run_effect(recompute);
        }
        let borrow = inner.borrow();
        match &borrow.state {
            MemoState::Clean(v) => f(v),
            MemoState::Dirty => panic!("memo has no value: its computation panicked"),
            MemoState::Computing => panic!("reactive cycle detected in memo"),
        }
    }

    /// Whether the storage behind this handle is still there — see [`crate::RwSignal::is_alive`].
    pub fn is_alive(&self) -> bool {
        runtime::signal_is_alive(self.id)
    }

    fn track(&self, inner: &Shared<T>) {
        if let Some(id) = runtime::current_observer() {
            let mut borrow = inner.borrow_mut();
            if !borrow.subscribers.contains(&id) {
                borrow.subscribers.push(id);
            }
        }
    }
}

impl<T: Clone + 'static> Memo<T> {
    pub fn get(&self) -> T {
        self.with(T::clone)
    }

    /// [`get`](Self::get), answering `None` rather than panicking when the storage is gone.
    pub fn try_get(&self) -> Option<T> {
        self.is_alive().then(|| self.get())
    }
}

/// A derived value recomputed when its sources change, notifying onward only when the result differs.
pub fn memo<T: PartialEq + 'static>(f: impl Fn() -> T + 'static) -> Memo<T> {
    let inner: Shared<T> = Rc::new(RefCell::new(MemoInner {
        state: MemoState::Dirty,
        subscribers: SmallVec::new(),
        recompute: None,
    }));

    // The arena slot owns the state, so the memo is disposed with its owner. The closure holds a `Weak` rather than the slot's id, because the recompute runs during a flush that may come after disposal — a `Weak` that fails to upgrade says so, where a stale id would only panic.
    let weak = Rc::downgrade(&inner);
    let effect_f: Box<dyn Fn()> = Box::new(move || {
        let Some(inner) = weak.upgrade() else {
            return;
        };
        // The previous value is held aside rather than overwritten: it is what the new one is compared against, and marking the state `Computing` in place threw it away, so every recompute notified onward whether or not anything changed.
        let previous = std::mem::replace(&mut inner.borrow_mut().state, MemoState::Computing);
        let new_value = match catch_unwind(AssertUnwindSafe(&f)) {
            Ok(value) => value,
            Err(payload) => {
                inner.borrow_mut().state = MemoState::Dirty;
                resume_unwind(payload);
            }
        };
        let subs: SmallVec<[EffectId; 8]> = {
            let mut memo = inner.borrow_mut();
            let changed = match &previous {
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

    let id = runtime::create_signal_storage(Rc::clone(&inner));
    // Registered as a pure effect, so it runs before user effects during flush. The owner active here recorded it, and that is what disposes it.
    let effect_id = runtime::register_pure_effect(effect_f);
    inner.borrow_mut().recompute = Some(effect_id);
    drop(inner);
    runtime::run_effect(effect_id);

    Memo {
        id,
        _marker: PhantomData,
    }
}
