//! The worker threads behind [`spawn_task`](super::spawn_task) and [`spawn_stream`](super::spawn_stream).
//!
//! Task work is expected to *block* — a query, a file read, a network call — so a fixed-size pool is the wrong shape: it deadlocks the moment as many tasks as there are workers sit waiting on I/O, and nothing can free one. This pool is elastic instead: a job goes to an idle thread if there is one, grows a new thread if there is not, and a thread that has been idle for [`KEEP_ALIVE`] retires. So the steady state for bursty small work is thread reuse, while blocking work still always makes progress.

use std::collections::VecDeque;
use std::sync::{Condvar, Mutex, MutexGuard, Once};
use std::time::Duration;

use crate::threads::{self, Spawned};

pub(in crate::task) type Job = Box<dyn FnOnce() + Send>;

/// How long a worker waits for another job before retiring.
const KEEP_ALIVE: Duration = Duration::from_secs(10);

/// Ceiling on live workers. Only reached when that many tasks block at once; jobs queue beyond it.
const MAX_WORKERS: usize = 512;

struct State {
    queue: VecDeque<Job>,
    idle: usize,
    // Retired threads stay here until the next `submit` reaps them, so this is an upper bound on live workers rather than an exact count — close enough for the growth decision, and it keeps `shutdown` able to join.
    workers: Vec<Spawned<()>>,
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
    static JOINED_BEFORE_UNMAP: Once = Once::new();
    JOINED_BEFORE_UNMAP.call_once(|| {
        // `on_unload` joins before `release_guest` frees the runtime and statics a running job may still reach; `on_quiesce` joins on a guest that panicked too, which is never told it is unloading.
        crate::thread_exit::on_unload(shutdown_and_join);
        crate::thread_exit::on_quiesce(shutdown_and_join);
    });
    let mut state = lock();
    let (retired, working): (Vec<_>, Vec<_>) = std::mem::take(&mut state.workers)
        .into_iter()
        .partition(Spawned::is_finished);
    state.workers = working;
    state.queue.push_back(job);
    if state.idle == 0 && state.workers.len() < MAX_WORKERS {
        match threads::spawn("telar-task", worker) {
            Ok(spawned) => state.workers.push(spawned),
            // Only fatal with nothing already running: otherwise a busy worker picks this job up when it frees.
            Err(e) => assert!(
                !state.workers.is_empty(),
                "cannot spawn a telar task thread: {e}"
            ),
        }
    }
    drop(state);
    POOL.work_ready.notify_one();
    // Joined rather than dropped: a retired worker may still be on its way out through this copy's code, which its host must not unmap under it.
    for worker in retired {
        let _ = worker.join();
    }
}

fn worker() {
    run_jobs();
    // By the thread itself, before it ends: app state is never freed from a thread-exit destructor, and a guest's thread registers none, so what its jobs left would otherwise outlive the library its host unloads after joining it.
    crate::thread_exit::release_thread();
}

fn run_jobs() {
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

/// Stops every worker and waits for the job each is running, so no pooled thread is left in code about to be unmapped, at the cost of a reload waiting on a job that never returns; it takes only poison-tolerant locks, the pool's and each dropped job's mailbox, and wakes the host's loop, so it is safe to run on a guest that panicked.
pub(in crate::task) fn shutdown_and_join() {
    lock().shutdown = true;
    loop {
        let (queued, handles) = {
            let mut state = lock();
            (
                std::mem::take(&mut state.queue),
                std::mem::take(&mut state.workers),
            )
        };
        if queued.is_empty() && handles.is_empty() {
            break;
        }
        // Dropped outside the lock: a queued job owns a `WorkerEnd` whose `Drop` posts to the mailbox and wakes the loop, neither of which may run while this mutex is held.
        drop(queued);
        POOL.work_ready.notify_all();
        for handle in handles {
            let _ = handle.join();
        }
    }
    lock().shutdown = false;
}

#[cfg(test)]
#[path = "native_test.rs"]
mod tests;
