//! Freeing what a thread holds when the thread exits, in a copy of the runtime its host may unload first.
//!
//! On glibc a thread-local whose type has a destructor pins the library that registered it until its thread exits, so one reached by a hot-reloaded app keeps that whole build mapped, code and statics, for the rest of the session. A thread-local slot is therefore declared without one — a `ManuallyDrop`, a raw pointer — and one holding enough to be worth freeing hands its release here instead. Where that release runs depends on the copy: an ordinary one runs it as the thread exits, from the one destructor this module registers per thread; one loaded as a guest registers none and leaves it to [`release_thread`], which its host calls before unloading it.

use std::cell::RefCell;
use std::mem::ManuallyDrop;
use std::sync::atomic::{AtomicBool, Ordering};

static LEFT_TO_HOST: AtomicBool = AtomicBool::new(false);

type Releases = Vec<fn()>;

crate::dropless_thread_local! {
    static RELEASES: ManuallyDrop<RefCell<Releases>> = const { ManuallyDrop::new(RefCell::new(Vec::new())) };
}

std::thread_local! {
    // The one destructor this module exists to register, which a copy told it is a guest never reaches: see `on_thread_exit`.
    static AT_EXIT: ReleaseAtExit = const { ReleaseAtExit };
}

struct ReleaseAtExit;

impl Drop for ReleaseAtExit {
    fn drop(&mut self) {
        release_thread();
    }
}

/// Runs `release` on this thread when it exits, or when [`release_thread`] does first. Call it once per value it frees: when the slot it empties goes from empty to filled.
pub fn on_thread_exit(release: fn()) {
    RELEASES.with(|releases| releases.borrow_mut().push(release));
    if !LEFT_TO_HOST.load(Ordering::Relaxed) {
        // Fails only once this thread's releases have run during its teardown: what is registered after that is leaked rather than panicking inside a destructor.
        let _ = AT_EXIT.try_with(|_| ());
    }
}

/// Runs every release registered on this thread and forgets it, those registered while they run included.
pub fn release_thread() {
    while let Some(release) = RELEASES.with(|releases| releases.borrow_mut().pop()) {
        release();
    }
}

/// Marks this copy of the runtime as a guest its host will unload: no thread-exit destructor is registered from now on, so what [`on_thread_exit`] is handed waits for [`release_thread`] instead.
pub fn leave_to_host() {
    LEFT_TO_HOST.store(true, Ordering::Relaxed);
}

#[cfg(test)]
#[path = "thread_exit_test.rs"]
mod tests;
