use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use super::*;

fn served() -> HostServices {
    HostServices {
        spawn_thread: Some(spawn_thread),
        join_thread: Some(join_thread),
        detach_thread: Some(detach_thread),
        ..HostServices::NONE
    }
}

#[test]
fn the_host_runs_the_body_on_a_thread_of_its_own_named_as_asked() {
    let table = served();
    let (send, ran) = mpsc::channel();
    let thread = unsafe {
        HostThread::start(
            &table,
            "telar-guest-task",
            Box::new(move || {
                let _ = send.send(std::thread::current().name().map(str::to_owned));
            }),
        )
    }
    .expect("the host starts it");
    unsafe { thread.join(&table) };
    assert_eq!(
        ran.try_recv().ok().flatten().as_deref(),
        Some("telar-guest-task"),
        "it ran, and had ended once joined"
    );
}

#[test]
fn joining_waits_for_the_body_to_end() {
    let table = served();
    let ended = Arc::new(AtomicBool::new(false));
    let marked = Arc::clone(&ended);
    let thread = unsafe {
        HostThread::start(
            &table,
            "telar-guest-task",
            Box::new(move || {
                std::thread::sleep(Duration::from_millis(30));
                marked.store(true, Ordering::Relaxed);
            }),
        )
    }
    .unwrap();
    unsafe { thread.join(&table) };
    assert!(ended.load(Ordering::Relaxed));
}

#[test]
fn a_body_that_panics_ends_its_thread_and_nothing_else() {
    let table = served();
    let thread = unsafe {
        HostThread::start(
            &table,
            "telar-guest-task",
            Box::new(|| panic!("the guest's body gave up")),
        )
    }
    .unwrap();
    unsafe { thread.join(&table) };
}

#[test]
fn a_detached_thread_runs_to_its_end_on_its_own() {
    let table = served();
    let (send, ran) = mpsc::channel();
    let thread = unsafe {
        HostThread::start(
            &table,
            "telar-guest-task",
            Box::new(move || {
                let _ = send.send(());
            }),
        )
    }
    .unwrap();
    unsafe { thread.detach(&table) };
    ran.recv_timeout(Duration::from_secs(5))
        .expect("the body ran");
}

#[test]
fn a_table_with_no_spawn_entry_starts_nothing_and_drops_the_body() {
    struct Dropped(Arc<AtomicBool>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Relaxed);
        }
    }
    let dropped = Arc::new(AtomicBool::new(false));
    let held = Dropped(Arc::clone(&dropped));
    let started = unsafe {
        HostThread::start(
            &HostServices::NONE,
            "telar-guest-task",
            Box::new(move || drop(held)),
        )
    };
    assert!(started.is_none());
    assert!(dropped.load(Ordering::Relaxed), "the body is freed unrun");
}
