//! [`Serial`]: one reconcile at a time, with the ones that arrive mid-reconcile queued behind it.

use std::cell::{Cell, RefCell};

/// Runs a reconcile to completion before the next one starts: a row's build can write a signal whose flush asks the same reconcile to start over, and running it there would reconcile against a list that is halfway through changing, so it is queued instead and applied once the running pass has committed.
pub(crate) struct Serial<T> {
    running: Cell<bool>,
    queued: RefCell<Option<T>>,
}

impl<T> Serial<T> {
    pub(crate) fn new() -> Self {
        Self {
            running: Cell::new(false),
            queued: RefCell::new(None),
        }
    }

    pub(crate) fn run(&self, value: T, mut apply: impl FnMut(T)) {
        if self.running.get() {
            drop(self.queued.replace(Some(value)));
            return;
        }
        self.running.set(true);
        let _idle = Idle(&self.running);
        // Left over only by a pass that panicked, and older than `value`.
        drop(self.queued.take());
        let mut next = Some(value);
        while let Some(value) = next {
            apply(value);
            next = self.queued.take();
        }
    }
}

struct Idle<'a>(&'a Cell<bool>);

impl Drop for Idle<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

#[cfg(test)]
#[path = "serial_test.rs"]
mod tests;
