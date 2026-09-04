//! The worker threads behind [`spawn_task`](super::spawn_task) and [`spawn_stream`](super::spawn_stream).
//!
//! Task work is expected to *block* — a query, a file read, a network call — so a fixed-size pool is the wrong shape: it deadlocks the moment as many tasks as there are workers sit waiting on I/O, and nothing can free one. This pool is elastic instead: a job goes to an idle thread if there is one, grows a new thread if there is not, and a thread that has been idle for [`KEEP_ALIVE`] retires. So the steady state for bursty small work is thread reuse, while blocking work still always makes progress.

use std::collections::VecDeque;
use std::sync::{Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

pub(in crate::task) type Job = Box<dyn FnOnce() + Send>;

/// How long a worker waits for another job before retiring.
const KEEP_ALIVE: Duration = Duration::from_secs(10);

/// Ceiling on live workers. Only reached when that many tasks block at once; jobs queue beyond it.
const MAX_WORKERS: usize = 512;

struct State {
    queue: VecDeque<Job>,
    idle: usize,
    // Retired threads stay here until the next `submit` reaps them, so this is an upper bound on live workers rather than an exact count — close enough for the growth decision, and it keeps `shutdown` able to join.
    workers: Vec<JoinHandle<()>>,
    shutdown: bool,
}

struct Pool {
    state: Mutex<State>,
    work_ready: Condvar,
}

static POOL: Pool = Pool {
    state: Mutex::new(State {
        queue: VecDeque::new(),
        idle: 0,
        workers: Vec::new(),
        shutdown: false,
    }),
    work_ready: Condvar::new(),
};

// A worker only moves jobs in and out, so a poisoned lock carries no torn state, and recovering keeps one panicking job from wedging every future task.
fn lock() -> MutexGuard<'static, State> {
    POOL.state.lock().unwrap_or_else(|e| e.into_inner())
}

pub(in crate::task) fn submit(job: Job) {
    let mut state = lock();
    state.workers.retain(|handle| !handle.is_finished());
    state.queue.push_back(job);
    if state.idle == 0 && state.workers.len() < MAX_WORKERS {
        match std::thread::Builder::new()
            .name("telar-task".to_string())
            .spawn(worker)
        {
            Ok(handle) => state.workers.push(handle),
            // Only fatal with nothing already running: otherwise a busy worker picks this job up when it frees.
            Err(e) => assert!(
                !state.workers.is_empty(),
                "cannot spawn a telar task thread: {e}"
            ),
        }
    }
    drop(state);
    POOL.work_ready.notify_one();
}

fn worker() {
    let mut state = lock();
    loop {
        if let Some(job) = state.queue.pop_front() {
            drop(state);
            // An unwinding job takes this thread with it. `WorkerEnd` has already released the task by then, and the next `submit` reaps the handle and grows a replacement.
            job();
            state = lock();
            continue;
        }
        if state.shutdown {
            return;
        }
        state.idle += 1;
        let (resumed, wait) = POOL
            .work_ready
            .wait_timeout(state, KEEP_ALIVE)
            .unwrap_or_else(|e| e.into_inner());
        state = resumed;
        state.idle -= 1;
        if wait.timed_out() && state.queue.is_empty() {
            return;
        }
    }
}

/// Stops every worker and waits for them, so no pooled thread is left parked in — or running — code that is about to be unmapped. Called before a hot-reload dylib is closed.
///
/// A job already running finishes first, and that is the point: its code lives in the dylib, so unloading underneath it is exactly the crash this prevents. The cost is that a reload waits for in-flight background work, and a task that blocks forever blocks the reload with it.
pub(in crate::task) fn shutdown_and_join() {
    let (queued, handles) = {
        let mut state = lock();
        state.shutdown = true;
        (
            std::mem::take(&mut state.queue),
            std::mem::take(&mut state.workers),
        )
    };
    // Dropped outside the lock: a queued job owns a `WorkerEnd` whose `Drop` posts to the mailbox and wakes the loop, neither of which may run while this mutex is held.
    drop(queued);
    POOL.work_ready.notify_all();
    for handle in handles {
        let _ = handle.join();
    }
    lock().shutdown = false;
}
