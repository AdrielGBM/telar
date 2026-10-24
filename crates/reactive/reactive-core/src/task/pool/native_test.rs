use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};

use super::*;
use crate::task::tests::serial;

fn flag() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

fn no_worker_left() -> bool {
    lock().workers.is_empty()
}

/// Submits a job that sleeps for `millis` once a worker has taken it, then sets the returned flag, and comes back once it is running.
fn run_sleeping(millis: u64) -> Arc<AtomicBool> {
    let finished = flag();
    let done = Arc::clone(&finished);
    let (running, taken) = mpsc::channel();
    submit(Box::new(move || {
        let _ = running.send(());
        std::thread::sleep(Duration::from_millis(millis));
        done.store(true, Ordering::Relaxed);
    }));
    taken.recv().expect("a worker takes the job");
    finished
}

#[test]
fn joining_waits_for_the_running_job_and_leaves_no_worker() {
    let _serial = serial();
    let finished = run_sleeping(50);
    shutdown_and_join();
    assert!(finished.load(Ordering::Relaxed), "the job finished first");
    assert!(no_worker_left());
}

#[test]
fn a_job_submitted_while_the_pool_is_joined_is_joined_too() {
    let _serial = serial();
    let started = flag();
    let finished = flag();
    let (start, done) = (Arc::clone(&started), Arc::clone(&finished));
    let (running, taken) = mpsc::channel();
    submit(Box::new(move || {
        let _ = running.send(());
        std::thread::sleep(Duration::from_millis(50));
        submit(Box::new(move || {
            start.store(true, Ordering::Relaxed);
            std::thread::sleep(Duration::from_millis(50));
            done.store(true, Ordering::Relaxed);
        }));
    }));
    taken.recv().expect("a worker takes the job");
    shutdown_and_join();
    assert!(no_worker_left(), "the worker grown for it is joined");
    assert!(
        !started.load(Ordering::Relaxed) || finished.load(Ordering::Relaxed),
        "it was dropped unrun, or ran to its end"
    );
}

/// A guest that panicked is quiesced all the same, so the join must not trip on a lock its panic poisoned.
#[test]
fn joining_survives_a_poisoned_pool_lock() {
    let _serial = serial();
    let poisoner = std::thread::spawn(|| {
        let _state = lock();
        panic!("a panic while the pool is locked");
    });
    assert!(poisoner.join().is_err());
    assert!(
        POOL.state.is_poisoned(),
        "precondition: the lock is poisoned"
    );
    let finished = run_sleeping(20);
    shutdown_and_join();
    assert!(finished.load(Ordering::Relaxed));
    assert!(no_worker_left());
}
