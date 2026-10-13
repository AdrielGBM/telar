use std::cell::Cell;

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
