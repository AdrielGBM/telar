use std::cell::Cell;
use std::sync::atomic::AtomicU32;

use super::*;

thread_local! {
    static RELEASED: Cell<u32> = const { Cell::new(0) };
}

fn count_release() {
    RELEASED.with(|released| released.set(released.get() + 1));
}

fn release_and_register_another() {
    count_release();
    on_thread_exit(count_release);
}

#[test]
fn a_release_runs_once_however_often_the_thread_is_released() {
    let released = std::thread::spawn(|| {
        on_thread_exit(count_release);
        release_thread();
        release_thread();
        RELEASED.with(Cell::get)
    })
    .join()
    .unwrap();
    assert_eq!(released, 1);
}

#[test]
fn a_release_registered_while_releasing_runs_too() {
    let released = std::thread::spawn(|| {
        on_thread_exit(release_and_register_another);
        release_thread();
        RELEASED.with(Cell::get)
    })
    .join()
    .unwrap();
    assert_eq!(released, 2);
}

#[test]
fn a_thread_that_exits_runs_what_it_registered() {
    static RAN: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    fn note() {
        RAN.fetch_add(1, Ordering::Relaxed);
    }
    std::thread::spawn(|| on_thread_exit(note)).join().unwrap();
    assert_eq!(RAN.load(Ordering::Relaxed), 1);
}

struct Counted(&'static AtomicU32);

impl Drop for Counted {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn a_released_slot_is_freed_when_its_thread_is_released_and_registers_again_once_refilled() {
    static DROPPED: AtomicU32 = AtomicU32::new(0);
    crate::dropless_thread_local! {
        static HELD: released Vec<Counted> = const { Vec::new() };
    }
    std::thread::spawn(|| {
        HELD.with(|held| held.borrow_mut().push(Counted(&DROPPED)));
        release_thread();
        assert_eq!(DROPPED.load(Ordering::Relaxed), 1);
        assert!(HELD.with(|held| held.borrow().is_empty()));

        HELD.with(|held| held.borrow_mut().push(Counted(&DROPPED)));
        release_thread();
        assert_eq!(DROPPED.load(Ordering::Relaxed), 2);
    })
    .join()
    .unwrap();
}

#[test]
fn a_released_slot_borrowed_while_its_thread_is_released_keeps_its_value() {
    static DROPPED: AtomicU32 = AtomicU32::new(0);
    crate::dropless_thread_local! {
        static HELD: released Option<Counted> = const { None };
    }
    std::thread::spawn(|| {
        HELD.with(|held| {
            *held.borrow_mut() = Some(Counted(&DROPPED));
            let _reading = held.borrow();
            release_thread();
        });
        assert_eq!(DROPPED.load(Ordering::Relaxed), 0);
        HELD.with(|held| held.borrow_mut().take());
        assert_eq!(DROPPED.load(Ordering::Relaxed), 1);
    })
    .join()
    .unwrap();
}

#[test]
fn a_destructor_that_refills_its_released_slot_is_released_in_the_same_pass() {
    static DROPPED: AtomicU32 = AtomicU32::new(0);
    struct Refills;
    impl Drop for Refills {
        fn drop(&mut self) {
            HELD.with(|held| held.borrow_mut().push(Box::new(Counted(&DROPPED))));
        }
    }
    crate::dropless_thread_local! {
        static HELD: released Vec<Box<dyn std::any::Any>> = const { Vec::new() };
    }
    std::thread::spawn(|| {
        HELD.with(|held| held.borrow_mut().push(Box::new(Refills)));
        release_thread();
        assert_eq!(DROPPED.load(Ordering::Relaxed), 1);
        assert!(HELD.with(|held| held.borrow().is_empty()));
    })
    .join()
    .unwrap();
}

/// Its drop glue is the app's code, which must not run inside a thread-local destructor, where a panic aborts the process: only an explicit release frees it.
#[test]
fn a_released_slot_is_left_as_its_thread_exits() {
    static DROPPED: AtomicU32 = AtomicU32::new(0);
    crate::dropless_thread_local! {
        static HELD: released Option<Counted> = const { None };
    }
    std::thread::spawn(|| HELD.with(|held| *held.borrow_mut() = Some(Counted(&DROPPED))))
        .join()
        .unwrap();
    assert_eq!(DROPPED.load(Ordering::Relaxed), 0);
}

/// An ordinary copy's statics live as long as the process, so nothing is kept to free them: only a copy left to its host is unloaded.
#[test]
fn a_copy_not_left_to_its_host_keeps_its_statics() {
    static RAN: AtomicU32 = AtomicU32::new(0);
    fn note() {
        RAN.fetch_add(1, Ordering::Relaxed);
    }
    on_unload(note);
    release_guest();
    assert_eq!(RAN.load(Ordering::Relaxed), 0);
}

#[test]
fn a_released_static_is_built_once_and_kept_by_a_copy_not_left_to_its_host() {
    static BUILT: AtomicU32 = AtomicU32::new(0);
    crate::released_static! {
        static CACHED: std::sync::Arc<u32> = std::sync::Arc::new(BUILT.fetch_add(1, Ordering::Relaxed));
    }
    let first = CACHED.get();
    release_guest();
    assert!(std::sync::Arc::ptr_eq(&first, &CACHED.get()));
    assert_eq!(BUILT.load(Ordering::Relaxed), 1);
}

thread_local! {
    static ORDER: std::cell::RefCell<Vec<&'static str>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn record(release: &'static str) {
    ORDER.with(|order| order.borrow_mut().push(release));
}

fn release_store() {
    record("store");
}

fn release_state() {
    record("state");
    on_release(release_state_filled_by_state);
}

fn release_state_filled_by_state() {
    record("state filled by state");
}

fn release_cache() {
    record("cache");
}

/// What the app's state holds is torn down into the stores it writes into, so those go last, after everything every other release registered too.
#[test]
fn the_last_releases_run_after_every_other_one() {
    let order = std::thread::spawn(|| {
        on_release_last(release_store);
        on_thread_exit(release_cache);
        on_release(release_state);
        release_thread();
        ORDER.with(|order| order.take())
    })
    .join()
    .unwrap();
    assert_eq!(order, ["state", "state filled by state", "cache", "store"]);
}
