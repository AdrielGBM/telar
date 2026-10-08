//! [`ActionLog`]: the calls a preview's callbacks received, for a panel to list and a play to count.

use std::collections::VecDeque;
use std::fmt;

use reactive_core::{RwSignal, signal};

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
    pub fn log(&self, name: &'static str, args: Vec<String>) {
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

#[cfg(test)]
#[path = "actions_test.rs"]
mod tests;
