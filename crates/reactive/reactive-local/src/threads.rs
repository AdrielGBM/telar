//! Starting a thread through the one hook every spawn of the runtime goes through, so a copy loaded as a guest has its host start the thread rather than its own std.
//!
//! std runs its spawn hooks on the thread that spawns, through a thread-local whose destructor keeps the library that reached it mapped until that thread exits. A guest spawning with its own std from its host's UI thread, which never exits, would keep every build its host unloads mapped for the rest of the session. So the runtime starts its threads with [`spawn`] and [`scope`], which a guest's installer routes to its host through [`set_spawner`], and which are std's own anywhere else.

use std::io;
use std::marker::PhantomData;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock};
use std::thread;

/// Starts threads in std's place: in a guest, its host, which runs each body on a thread its own std spawned.
///
/// # Safety
///
/// A body is run exactly once on the thread it starts, or dropped unrun when `spawn` answers an error, and [`Joinable::join`] returns only once that thread has ended: [`scope`] lends its threads borrows that end when it returns.
pub unsafe trait Spawner: Sync {
    /// Starts a thread named `name` running `main`, or says why it could not.
    fn spawn(&self, name: String, main: Box<dyn FnOnce() + Send>) -> io::Result<Box<dyn Joinable>>;
}

/// A thread a [`Spawner`] started. Dropping it without joining leaves the thread to run on its own.
pub trait Joinable: Send {
    /// Waits for the thread to end.
    fn join(self: Box<Self>);
}

static SPAWNER: RwLock<Option<&'static dyn Spawner>> = RwLock::new(None);

/// Starts every later [`spawn`] and [`scope`] thread of this copy through `spawner`.
///
/// A `'static` borrow rather than a box, so it holds nothing a host's unload has to free, and a thread started while a guest is being released still starts on its host.
pub fn set_spawner(spawner: &'static dyn Spawner) {
    *SPAWNER.write().unwrap_or_else(PoisonError::into_inner) = Some(spawner);
}

fn spawner() -> Option<&'static dyn Spawner> {
    *SPAWNER.read().unwrap_or_else(PoisonError::into_inner)
}

type Outcome<T> = Arc<Mutex<Option<thread::Result<T>>>>;

fn lock<T>(outcome: &Outcome<T>) -> MutexGuard<'_, Option<thread::Result<T>>> {
    outcome.lock().unwrap_or_else(PoisonError::into_inner)
}

fn take<T>(outcome: &Outcome<T>) -> thread::Result<T> {
    lock(outcome)
        .take()
        .unwrap_or_else(|| Err(Box::new("the thread ended without running its body")))
}

struct StdThread(thread::JoinHandle<()>);

impl Joinable for StdThread {
    fn join(self: Box<Self>) {
        let _ = self.0.join();
    }
}

/// Starts `main` on a thread through the installed spawner, or std's, keeping what it returns or the panic it unwinds with.
///
/// # Safety
///
/// The thread must be joined before anything `main` or `T` borrows for `'a` ends.
unsafe fn start<'a, T: Send + 'a>(
    name: String,
    main: impl FnOnce() -> T + Send + 'a,
) -> io::Result<(Box<dyn Joinable>, Outcome<T>)> {
    let outcome: Outcome<T> = Arc::new(Mutex::new(None));
    let filled = Arc::clone(&outcome);
    let body: Box<dyn FnOnce() + Send + 'a> = Box::new(move || {
        let returned = catch_unwind(AssertUnwindSafe(main));
        *lock(&filled) = Some(returned);
    });
    // Outlived by nothing it borrows: the caller joins the thread before `'a` ends.
    let body = unsafe {
        std::mem::transmute::<Box<dyn FnOnce() + Send + 'a>, Box<dyn FnOnce() + Send + 'static>>(
            body,
        )
    };
    let thread: Box<dyn Joinable> = match spawner() {
        Some(spawner) => spawner.spawn(name, body)?,
        None => Box::new(StdThread(thread::Builder::new().name(name).spawn(body)?)),
    };
    Ok((thread, outcome))
}

/// A thread [`spawn`] started, and what its body returned once it has. Dropping it leaves the thread to run on its own.
pub struct Spawned<T> {
    thread: Box<dyn Joinable>,
    outcome: Outcome<T>,
}

impl<T> Spawned<T> {
    /// Whether the body has returned or unwound. The thread may still be on its way out, so it is [joined](Self::join) before the code it ran may be unmapped.
    pub fn is_finished(&self) -> bool {
        lock(&self.outcome).is_some()
    }

    /// Waits for the thread to end, and answers what its body returned or the panic it unwound with.
    pub fn join(self) -> thread::Result<T> {
        self.thread.join();
        take(&self.outcome)
    }
}

/// Starts `main` on a thread named `name`, as `std::thread::Builder::spawn` does, through this copy's [`Spawner`] when one is installed.
pub fn spawn<T: Send + 'static>(
    name: impl Into<String>,
    main: impl FnOnce() -> T + Send + 'static,
) -> io::Result<Spawned<T>> {
    let (thread, outcome) = unsafe { start(name.into(), main) }?;
    Ok(Spawned { thread, outcome })
}

/// Runs `body` with a [`Scope`] whose threads may borrow what outlives this call, and returns once every thread it started has ended, as `std::thread::scope` does. A thread's panic reaches only whoever joins it.
pub fn scope<'env, R>(body: impl for<'scope> FnOnce(&'scope Scope<'env>) -> R) -> R {
    let scope = Scope {
        threads: Mutex::new(Threads::default()),
        _env: PhantomData,
    };
    let _join = JoinTheRest(&scope);
    body(&scope)
}

#[derive(Default)]
struct Threads {
    started: usize,
    running: Vec<(usize, Box<dyn Joinable>)>,
}

/// Where [`scope`] keeps the threads it waits for.
pub struct Scope<'env> {
    threads: Mutex<Threads>,
    // Invariant, so a thread is never lent a borrow shorter than the call to `scope`.
    _env: PhantomData<&'env mut &'env ()>,
}

impl<'env> Scope<'env> {
    /// Starts `main` on a thread named `name`, which may borrow anything that outlives the [`scope`] call.
    pub fn spawn<'scope, T: Send + 'env>(
        &'scope self,
        name: impl Into<String>,
        main: impl FnOnce() -> T + Send + 'env,
    ) -> io::Result<ScopedSpawned<'scope, 'env, T>> {
        let mut threads = self.lock();
        let (thread, outcome) = unsafe { start(name.into(), main) }?;
        let id = threads.started;
        threads.started += 1;
        threads.running.push((id, thread));
        Ok(ScopedSpawned {
            scope: self,
            id,
            outcome,
        })
    }

    fn lock(&self) -> MutexGuard<'_, Threads> {
        self.threads.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn take(&self, id: usize) -> Option<Box<dyn Joinable>> {
        let mut threads = self.lock();
        let index = threads.running.iter().position(|(held, _)| *held == id)?;
        Some(threads.running.swap_remove(index).1)
    }
}

/// A thread a [`Scope`] started, and what its body returned once it has.
pub struct ScopedSpawned<'scope, 'env, T> {
    scope: &'scope Scope<'env>,
    id: usize,
    outcome: Outcome<T>,
}

impl<T> ScopedSpawned<'_, '_, T> {
    /// Waits for the thread to end, and answers what its body returned or the panic it unwound with.
    pub fn join(self) -> thread::Result<T> {
        if let Some(thread) = self.scope.take(self.id) {
            thread.join();
        }
        take(&self.outcome)
    }
}

/// Joins what a [`scope`] left running, on an unwind out of its body too, and what those threads started in turn.
struct JoinTheRest<'scope, 'env>(&'scope Scope<'env>);

impl Drop for JoinTheRest<'_, '_> {
    fn drop(&mut self) {
        loop {
            let running = std::mem::take(&mut self.0.lock().running);
            if running.is_empty() {
                break;
            }
            for (_, thread) in running {
                thread.join();
            }
        }
    }
}

#[cfg(test)]
#[path = "threads_test.rs"]
mod tests;
