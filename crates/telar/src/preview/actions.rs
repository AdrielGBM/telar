//! [`ActionLog`]: the calls a preview's callbacks received, for a panel to list and a play to count. [`IntoAction`] makes the callbacks that record them, and [`PreviewActions`] hands such callbacks to a component's props.

use std::any::{Any, type_name};
use std::collections::VecDeque;
use std::fmt::{self, Debug};
use std::rc::Rc;

use reactive_core::{RwSignal, signal};

use super::__probe;

/// One call a preview's callback received.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionCall {
    /// 1-based, and never reused for as long as the log lives, cleared or not: what a panel keys its rows by.
    pub seq: u64,
    /// The callback's name, as the prop it was passed to is named.
    pub name: &'static str,
    /// Each argument's `Debug` text, in order.
    pub args: Vec<String>,
}

impl ActionCall {
    pub fn new(seq: u64, name: &'static str, args: Vec<String>) -> Self {
        Self { seq, name, args }
    }
}

/// The calls a canvas's callbacks received, newest last, keeping only the most recent [`capacity`](Self::capacity) of them.
///
/// A cheap `Copy` handle over one signal: the canvas, its callbacks and a panel each hold one, and a panel that reads it re-renders on each call. The signal belongs to the owner that was active when the log was made.
#[derive(Clone, Copy)]
pub struct ActionLog {
    ring: RwSignal<Ring>,
    capacity: usize,
}

#[derive(Default)]
struct Ring {
    calls: VecDeque<ActionCall>,
    logged: u64,
    last_seq: u64,
}

impl ActionLog {
    pub const DEFAULT_CAPACITY: usize = 200;

    pub fn new() -> Self {
        Self::with_capacity(Self::DEFAULT_CAPACITY)
    }

    /// A log keeping the last `capacity` calls, and at least one.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            ring: signal(Ring::default()),
            capacity: capacity.max(1),
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Records a call to `name` with `args`, dropping the oldest call kept once the log is full.
    ///
    /// A log whose owner is gone records nothing: a callback can outlive the canvas it was made for, and its call is not the preview's to report any more.
    pub fn log(&self, name: &'static str, args: Vec<String>) {
        if !self.ring.is_alive() {
            return;
        }
        let capacity = self.capacity;
        self.ring.update(|ring| {
            ring.last_seq += 1;
            ring.logged += 1;
            if ring.calls.len() == capacity {
                ring.calls.pop_front();
            }
            ring.calls
                .push_back(ActionCall::new(ring.last_seq, name, args));
        });
    }

    /// The calls kept, oldest first. Reactive.
    pub fn calls(&self) -> Vec<ActionCall> {
        self.ring.with(|ring| ring.calls.iter().cloned().collect())
    }

    /// The most recent call. Reactive.
    pub fn last(&self) -> Option<ActionCall> {
        self.ring.with(|ring| ring.calls.back().cloned())
    }

    /// How many calls were logged since the last [`clear`](Self::clear), the ones the ring has dropped included. Reactive.
    pub fn count(&self) -> u64 {
        self.ring.with(|ring| ring.logged)
    }

    /// How many of the calls kept went to `name`. Reactive.
    pub fn count_of(&self, name: &str) -> usize {
        self.ring
            .with(|ring| ring.calls.iter().filter(|call| call.name == name).count())
    }

    pub fn clear(&self) {
        self.ring.update(|ring| {
            ring.calls.clear();
            ring.logged = 0;
        });
    }
}

impl Default for ActionLog {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for ActionLog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.ring.peek_with(|ring| {
            f.debug_struct("ActionLog")
                .field("capacity", &self.capacity)
                .field("logged", &ring.logged)
                .field("calls", &ring.calls)
                .finish()
        })
    }
}

/// A callback whose calls a preview records: an `Rc<dyn Fn(…)>` or a `Box<dyn Fn(…)>` of up to three arguments that returns nothing, or an `Option` of one.
///
/// Each argument is recorded by its `Debug` text. The code `#[derive(Props)]` generates also logs a callback of one borrowed argument, `Rc<dyn Fn(&T)>`, and one whose arguments have no `Debug`, recording those by their type's name, and leaves any other field as it is.
pub trait IntoAction: Sized {
    /// A callback that does nothing but record each call to `name` in `log`.
    fn action(log: ActionLog, name: &'static str) -> Self;

    /// This callback, recording each call to `name` in `log` before it runs. An `Option` holding none becomes [`action`](Self::action).
    fn logged(self, log: ActionLog, name: &'static str) -> Self;
}

/// Props whose callbacks a preview can log, each under its prop's name: what `#[derive(Props)]` implements in a build with previews.
///
/// [`preview!`](crate::preview::preview!) applies it to the props of every call its body makes to the component under preview, and the props builder's `__preview_actions` does the same to the optional props it holds, for generated code that has a builder in hand.
pub trait PreviewActions: Sized {
    /// These props with every callback logged in `log`: one left unset does nothing but log, and one set logs, then runs.
    fn preview_actions(self, log: &ActionLog) -> Self;
}

fn debug_text<T: Debug>(value: &T) -> String {
    format!("{value:?}")
}

fn type_text<T>(_: &T) -> String {
    format!("<{}>", type_name::<T>())
}

macro_rules! callbacks {
    ($trait:path, $describe:ident, <$($generic:ident: [$($bound:tt)+]),*>, ($($arg:ident: $ty:ty),*)) => {
        callbacks!(@pointer Rc, $trait, $describe, <$($generic: [$($bound)+]),*>, ($($arg: $ty),*));
        callbacks!(@pointer Box, $trait, $describe, <$($generic: [$($bound)+]),*>, ($($arg: $ty),*));
    };
    (@pointer $pointer:ident, $trait:path, $describe:ident, <$($generic:ident: [$($bound:tt)+]),*>, ($($arg:ident: $ty:ty),*)) => {
        impl<$($generic: $($bound)+ + 'static),*> $trait for $pointer<dyn Fn($($ty),*)> {
            fn action(log: ActionLog, name: &'static str) -> Self {
                $pointer::new(move |$($arg: $ty),*| log.log(name, vec![$($describe(&$arg)),*]))
            }

            fn logged(self, log: ActionLog, name: &'static str) -> Self {
                let callback = self;
                $pointer::new(move |$($arg: $ty),*| {
                    log.log(name, vec![$($describe(&$arg)),*]);
                    callback($($arg),*)
                })
            }
        }
    };
}

callbacks!(IntoAction, debug_text, <>, ());
callbacks!(IntoAction, debug_text, <A: [Debug]>, (a: A));
callbacks!(IntoAction, debug_text, <A: [Debug], B: [Debug]>, (a: A, b: B));
callbacks!(IntoAction, debug_text, <A: [Debug], B: [Debug], C: [Debug]>, (a: A, b: B, c: C));
callbacks!(__probe::IntoOpaqueAction, type_text, <>, ());
callbacks!(__probe::IntoOpaqueAction, type_text, <A: [Any]>, (a: A));
callbacks!(__probe::IntoOpaqueAction, type_text, <A: [Any], B: [Any]>, (a: A, b: B));
callbacks!(__probe::IntoOpaqueAction, type_text, <A: [Any], B: [Any], C: [Any]>, (a: A, b: B, c: C));
callbacks!(__probe::IntoBorrowedAction, debug_text, <A: [Debug + ?Sized]>, (a: &A));
callbacks!(__probe::IntoOpaqueBorrowedAction, type_text, <A: [?Sized]>, (a: &A));

macro_rules! optional_callbacks {
    ($($trait:path),*) => {
        $(
            impl<F: $trait> $trait for Option<F> {
                fn action(log: ActionLog, name: &'static str) -> Self {
                    Some(F::action(log, name))
                }

                fn logged(self, log: ActionLog, name: &'static str) -> Self {
                    Some(match self {
                        Some(callback) => callback.logged(log, name),
                        None => F::action(log, name),
                    })
                }
            }
        )*
    };
}

optional_callbacks!(
    IntoAction,
    __probe::IntoOpaqueAction,
    __probe::IntoBorrowedAction,
    __probe::IntoOpaqueBorrowedAction
);

#[cfg(test)]
#[path = "actions_test.rs"]
mod tests;
