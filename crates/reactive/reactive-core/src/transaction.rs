//! [`Transaction`]: one gesture's edit of a signal, previewed live and then committed or reverted as a whole.

use std::fmt;
use std::marker::PhantomData;
use std::rc::Rc;

use crate::RwSignal;
use crate::runtime::{self, SignalId};

/// Why a [`Transaction`] call did nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransactionError {
    /// A transaction is already open on this signal, this one or another. Transactions do not nest: the outer one owns the snapshot, and a second snapshot taken mid-preview would make "revert" mean two different states.
    AlreadyOpen,
    /// Nothing is open to preview into, commit or revert. A second `commit` or `revert` lands here, which is what makes each one happen exactly once.
    NotOpen,
    /// The signal, or the owner the transaction belongs to, has been disposed.
    Disposed,
}

impl fmt::Display for TransactionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::AlreadyOpen => "a transaction is already open on this signal",
            Self::NotOpen => "no transaction is open",
            Self::Disposed => "the transaction or its signal has been disposed",
        })
    }
}

impl std::error::Error for TransactionError {}

struct Open<T> {
    before: T,
    seen: u64,
    touched: bool,
}

type CommitListener<T> = Rc<dyn Fn(&T, &T)>;

struct State<T: 'static> {
    open: Option<Open<T>>,
    on_commit: Option<CommitListener<T>>,
}

/// A gesture's edit of an [`RwSignal`], previewed live via [`preview`](Self::preview) and closed by exactly one [`commit`](Self::commit) or [`revert`](Self::revert): only one may be open per signal at a time, a write from elsewhere while it's open moves the snapshot forward instead of being clobbered, and disposing the owner active at [`new`](Self::new) reverts it mid-gesture.
pub struct Transaction<T: 'static> {
    target: RwSignal<T>,
    state: SignalId,
    _marker: PhantomData<T>,
}

impl<T: 'static> Clone for Transaction<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> Copy for Transaction<T> {}

impl<T: Clone + 'static> Transaction<T> {
    pub fn new(target: RwSignal<T>) -> Self {
        let state = runtime::create_signal_storage(State::<T> {
            open: None,
            on_commit: None,
        });
        let transaction = Self {
            target,
            state,
            _marker: PhantomData,
        };
        crate::on_cleanup(move || {
            let _ = transaction.revert();
        });
        transaction
    }

    /// Called with `(before, after)` on every commit, whoever asked for it — a drag's release, a popover's outside click, or a direct [`commit`](Self::commit). Untracked, so reading signals here subscribes nothing.
    pub fn on_commit(self, f: impl Fn(&T, &T) + 'static) -> Self {
        let f: CommitListener<T> = Rc::new(f);
        let replaced = self.with_state(|s| s.on_commit.replace(f));
        drop(replaced);
        self
    }

    pub fn signal(&self) -> RwSignal<T> {
        self.target
    }

    pub fn is_open(&self) -> bool {
        self.with_state(|s| s.open.is_some()).unwrap_or(false)
    }

    /// The snapshot a preview is measured from, or `None` when nothing is open.
    pub fn before(&self) -> Option<T> {
        self.sync().ok()?;
        self.with_state(|s| s.open.as_ref().map(|o| o.before.clone()))
            .flatten()
    }

    /// Snapshots the signal and opens the transaction.
    pub fn begin(&self) -> Result<(), TransactionError> {
        let seen = runtime::signal_version(self.target.id).ok_or(TransactionError::Disposed)?;
        if self
            .with_state(|s| s.open.is_some())
            .ok_or(TransactionError::Disposed)?
            || !runtime::claim_transaction(self.target.id)
        {
            return Err(TransactionError::AlreadyOpen);
        }
        let before = self.target.peek();
        self.with_state(|s| {
            s.open = Some(Open {
                before,
                seen,
                touched: false,
            })
        })
        .ok_or(TransactionError::Disposed)
    }

    /// Writes `f`'s change to the signal, live and unrecorded.
    pub fn preview(&self, f: impl FnOnce(&mut T)) -> Result<(), TransactionError> {
        self.sync()?;
        self.target.update(f);
        let seen = runtime::signal_version(self.target.id);
        self.with_state(|s| {
            if let (Some(open), Some(seen)) = (&mut s.open, seen) {
                open.seen = seen;
                open.touched = true;
            }
        });
        Ok(())
    }

    /// Closes the transaction keeping what was previewed, and answers `(before, after)`.
    pub fn commit(&self) -> Result<(T, T), TransactionError> {
        let open = self.close()?;
        let after = self.target.peek();
        if let Some(listener) = self.with_state(|s| s.on_commit.clone()).flatten() {
            runtime::untracked(|| listener(&open.before, &after));
        }
        Ok((open.before, after))
    }

    /// Closes the transaction putting the snapshot back.
    pub fn revert(&self) -> Result<(), TransactionError> {
        let open = self.close()?;
        if open.touched {
            self.target.set(open.before);
        }
        Ok(())
    }

    fn close(&self) -> Result<Open<T>, TransactionError> {
        self.sync()?;
        let open = self
            .with_state(|s| s.open.take())
            .flatten()
            .ok_or(TransactionError::NotOpen)?;
        runtime::release_transaction(self.target.id);
        Ok(open)
    }

    /// Rebases the snapshot onto a write this transaction did not make, and closes it when the signal is gone.
    fn sync(&self) -> Result<(), TransactionError> {
        let open = self
            .with_state(|s| s.open.as_ref().map(|o| o.seen))
            .ok_or(TransactionError::Disposed)?;
        let Some(seen) = open else {
            return Err(TransactionError::NotOpen);
        };
        let Some(version) = runtime::signal_version(self.target.id) else {
            let abandoned = self.with_state(|s| s.open.take());
            runtime::release_transaction(self.target.id);
            drop(abandoned);
            return Err(TransactionError::Disposed);
        };
        if version != seen {
            let now = self.target.peek();
            let stale = self.with_state(|s| {
                s.open.as_mut().map(|open| {
                    open.seen = version;
                    open.touched = false;
                    std::mem::replace(&mut open.before, now)
                })
            });
            drop(stale);
        }
        Ok(())
    }

    /// Values are handed out rather than dropped in here: this runs under the runtime's borrow, and a value that owns signal handles re-enters the runtime as it drops.
    fn with_state<R>(&self, f: impl FnOnce(&mut State<T>) -> R) -> Option<R> {
        runtime::try_update_signal_value::<State<T>, R>(self.state, f)
    }
}

#[cfg(test)]
#[path = "transaction_test.rs"]
mod tests;
