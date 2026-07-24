use std::sync::{Arc, RwLock};

/// A process-global "wake the UI event loop" handle.
type Waker = Arc<dyn Fn() + Send + Sync>;

static LOOP_WAKER: RwLock<Option<Waker>> = RwLock::new(None);

/// Registers the process-global loop wake, installed once by the platform at startup. Waking it must run a
/// frame for **every** live surface — so an app's `on_frame` runs wherever its content currently lives, even
/// after a tabbed host has moved it to another window — and must keep **no** window alive. This is what lets
/// an app cache its redraw waker (or hand a clone to a worker thread) with zero per-window bookkeeping.
pub fn set_loop_waker(waker: Waker) {
    *LOOP_WAKER.write().unwrap() = Some(waker);
}

/// The process-global loop wake, if the platform installed one. `None` on backends that only expose a
/// per-window redraw (the caller then falls back to a window-bound waker).
pub fn loop_waker() -> Option<Waker> {
    LOOP_WAKER.read().unwrap().clone()
}
