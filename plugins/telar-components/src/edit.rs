use std::cell::Cell;
use std::rc::Rc;

use telar::Transaction;

/// If `transaction` is already open (a drag or popover in progress), previews `next` on it instead of starting a new commit, so a concurrent transaction on the same signal rebases onto it.
pub(crate) fn write_through<T: Clone + PartialEq + 'static>(transaction: Transaction<T>, next: T) {
    if transaction.is_open() {
        let _ = transaction.preview(|value| *value = next);
        return;
    }
    if transaction.signal().peek() == next {
        return;
    }
    if transaction.begin().is_ok() {
        let _ = transaction.preview(|value| *value = next);
        let _ = transaction.commit();
    } else {
        transaction.signal().set(next);
    }
}

pub(crate) fn snap(raw: f32, increment: f32, min: f32, max: f32) -> f32 {
    let snapped = if increment > 0.0 {
        (raw / increment).round() * increment
    } else {
        raw
    };
    snapped.clamp(min, max)
}

/// A drag under way, so a widget's `on_end` answers each `on_start` exactly once: at the release, at a cancel, or — when neither comes because the widget is gone — as the last closure holding it drops.
pub(crate) struct Stroke {
    on_start: Option<Rc<dyn Fn()>>,
    on_end: Option<Rc<dyn Fn(bool)>>,
    live: Cell<bool>,
}

impl Stroke {
    pub(crate) fn new(on_start: Option<Rc<dyn Fn()>>, on_end: Option<Rc<dyn Fn(bool)>>) -> Self {
        Self {
            on_start,
            on_end,
            live: Cell::new(false),
        }
    }

    pub(crate) fn start(&self) {
        if !self.live.replace(true)
            && let Some(on_start) = &self.on_start
        {
            on_start();
        }
    }

    pub(crate) fn end(&self, kept: bool) {
        if self.live.replace(false)
            && let Some(on_end) = &self.on_end
        {
            on_end(kept);
        }
    }
}

impl Drop for Stroke {
    fn drop(&mut self) {
        self.end(false);
    }
}
