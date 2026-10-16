//! Freeing what a thread holds, in a copy of the runtime its host may unload before the thread exits.
//!
//! On glibc a thread-local whose type has a destructor pins the library that registered it until its thread exits, so one reached by a hot-reloaded app keeps that whole build mapped, code and statics, for the rest of the session. A thread-local slot is therefore declared without one — a [`Released`], a `ManuallyDrop`, a raw pointer — and one holding heap data hands its release here instead.
//!
//! Two kinds of release, by when they may run. A cache of plain data, handed to [`on_thread_exit`], is freed as its thread exits, from the one destructor this module registers per thread, or when [`release_thread`] runs first. The app's own state, handed to [`on_release`] and [`on_release_last`], is freed only by [`release_thread`]: dropping it runs the app's destructors, and one that panics inside a thread-local destructor aborts the process, so a thread that simply exits leaves it, as a process that exits leaves its memory. A copy loaded as a guest registers no destructor at all, and its host calls [`release_guest`] before unloading it, which frees both.
//!
//! A static is never dropped, which in an ordinary copy is what the process wants, since it lives as long as the process does. A guest's statics are left behind when its library is unmapped, so a guest hands what one holds to [`on_unload`] as well.

use std::cell::{Cell, RefCell};
use std::mem::ManuallyDrop;
use std::ops::Deref;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};

static LEFT_TO_HOST: AtomicBool = AtomicBool::new(false);

static UNLOADS: Mutex<Vec<fn()>> = Mutex::new(Vec::new());

#[derive(Default)]
struct Releases {
    state: Vec<fn()>,
    at_exit: Vec<fn()>,
    last: Vec<fn()>,
}

impl Releases {
    /// The next to run: the app's state first, so what its destructors reach for is still there, then the caches it may have filled again, then the stores everything else writes into.
    fn next(&mut self) -> Option<fn()> {
        self.state
            .pop()
            .or_else(|| self.at_exit.pop())
            .or_else(|| self.last.pop())
    }

    fn is_empty(&self) -> bool {
        self.state.is_empty() && self.at_exit.is_empty() && self.last.is_empty()
    }
}

crate::dropless_thread_local! {
    static RELEASES: ManuallyDrop<RefCell<Releases>> = const {
        ManuallyDrop::new(RefCell::new(Releases {
            state: Vec::new(),
            at_exit: Vec::new(),
            last: Vec::new(),
        }))
    };
}

std::thread_local! {
    // The one destructor this module exists to register, which a copy told it is a guest never reaches: see `on_thread_exit`.
    static AT_EXIT: ReleaseAtExit = const { ReleaseAtExit };
}

struct ReleaseAtExit;

impl Drop for ReleaseAtExit {
    fn drop(&mut self) {
        while let Some(release) = RELEASES.with(|releases| releases.borrow_mut().at_exit.pop()) {
            release();
        }
    }
}

/// Runs `release` on this thread when it exits, or when [`release_thread`] does first: for a cache of plain data, which is safe to free from a thread-local destructor. Call it once per value it frees: when the slot it empties goes from empty to filled.
pub fn on_thread_exit(release: fn()) {
    RELEASES.with(|releases| releases.borrow_mut().at_exit.push(release));
    if !LEFT_TO_HOST.load(Ordering::Relaxed) {
        // Fails only once this thread's releases have run during its teardown: what is registered after that is leaked rather than panicking inside a destructor.
        let _ = AT_EXIT.try_with(|_| ());
    }
}

/// Runs `release` when [`release_thread`] does, and not as the thread exits: for the app's state, whose destructors are the app's code. Call it once per value it frees, as [`on_thread_exit`].
pub fn on_release(release: fn()) {
    RELEASES.with(|releases| releases.borrow_mut().state.push(release));
}

/// [`on_release`] for a store the others' destructors write into, the reactive runtime: it runs once every other release has, those they registered included.
pub fn on_release_last(release: fn()) {
    RELEASES.with(|releases| releases.borrow_mut().last.push(release));
}

/// Runs every release registered on this thread and forgets it, those registered while they run included.
pub fn release_thread() {
    while let Some(release) = RELEASES.with(|releases| releases.borrow_mut().next()) {
        release();
    }
    RELEASES.with(|releases| drop(std::mem::take(&mut *releases.borrow_mut())));
}

/// Runs `release` when the host unloads this copy, for what a static holds. Call it when the static goes from empty to filled. An ordinary copy keeps its statics for as long as the process runs, so there it registers nothing.
pub fn on_unload(release: fn()) {
    if LEFT_TO_HOST.load(Ordering::Relaxed) {
        unloads().push(release);
    }
}

fn unloads() -> std::sync::MutexGuard<'static, Vec<fn()>> {
    UNLOADS.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What a host calls on the thread it drove a guest from, right before it unloads it: runs what [`on_unload`] was handed, then what this thread registered, until neither has anything left, since freeing one can fill the other again.
pub fn release_guest() {
    loop {
        let unload = unloads().pop();
        match unload {
            Some(release) => release(),
            None if RELEASES.with(|releases| releases.borrow().is_empty()) => break,
            None => release_thread(),
        }
    }
    drop(std::mem::take(&mut *unloads()));
}

/// Marks this copy of the runtime as a guest its host will unload: no thread-exit destructor is registered from now on, so what [`on_thread_exit`] is handed waits for [`release_guest`] instead, and what [`on_unload`] is handed is kept for it.
pub fn leave_to_host() {
    LEFT_TO_HOST.store(true, Ordering::Relaxed);
}

/// The slot `dropless_thread_local!` declares for `static NAME: released T = init;`: a `RefCell<T>` that registers its own release with [`on_release`] the first time it is reached, so what it holds is freed when its thread is released, as a host releases a guest's before unloading it.
///
/// Released into a fresh `init` rather than left empty, so a destructor its release runs that reaches the slot again finds a value to use rather than a freed one; reaching it registers that value's release in turn.
pub struct Released<T> {
    value: ManuallyDrop<RefCell<T>>,
    fresh: fn() -> T,
    release_slot: fn(),
    registered: Cell<bool>,
}

impl<T> Released<T> {
    #[doc(hidden)]
    pub const fn new(value: T, fresh: fn() -> T, release_slot: fn()) -> Self {
        Self {
            value: ManuallyDrop::new(RefCell::new(value)),
            fresh,
            release_slot,
            registered: Cell::new(false),
        }
    }

    /// Swaps what the slot holds for a fresh value and drops it, outside the borrow so its destructor may reach the slot. A slot borrowed while it is released keeps its value: a borrow cannot be outlived.
    #[doc(hidden)]
    pub fn release(&self) {
        self.registered.set(false);
        let Ok(mut value) = self.value.try_borrow_mut() else {
            return;
        };
        let held = std::mem::replace(&mut *value, (self.fresh)());
        drop(value);
        drop(held);
    }
}

impl<T> Deref for Released<T> {
    type Target = RefCell<T>;

    fn deref(&self) -> &RefCell<T> {
        if !self.registered.replace(true) {
            on_release(self.release_slot);
        }
        &self.value
    }
}

#[cfg(test)]
#[path = "thread_exit_test.rs"]
mod tests;
