use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use super::*;

#[test]
fn a_thread_answers_what_its_body_returned_once_joined() {
    let spawned = spawn("telar-test", || 6 * 7).expect("the thread starts");
    assert_eq!(spawned.join().ok(), Some(42));
}

#[test]
fn a_thread_whose_body_panics_answers_the_panic() {
    let spawned = spawn("telar-test", || -> u8 { panic!("the body gave up") }).unwrap();
    let panic = spawned.join().expect_err("the body unwound");
    assert_eq!(panic.downcast_ref::<&str>(), Some(&"the body gave up"));
}

#[test]
fn a_thread_is_finished_once_its_body_has_returned() {
    let (release, released) = std::sync::mpsc::channel::<()>();
    let spawned = spawn("telar-test", move || released.recv().is_ok()).unwrap();
    assert!(!spawned.is_finished(), "its body is still waiting");
    release.send(()).unwrap();
    while !spawned.is_finished() {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(spawned.join().ok(), Some(true));
}

#[test]
fn a_thread_is_named_as_asked() {
    let name = spawn("telar-named", || {
        std::thread::current().name().map(str::to_owned)
    })
    .unwrap()
    .join()
    .unwrap();
    assert_eq!(name.as_deref(), Some("telar-named"));
}

#[test]
fn a_scope_lends_its_threads_borrows_and_waits_for_the_ones_nobody_joined() {
    let finished = AtomicUsize::new(0);
    let words = ["one", "two", "three"];
    let lengths = scope(|scope| {
        for _ in 0..4 {
            scope
                .spawn("telar-scoped", || {
                    std::thread::sleep(Duration::from_millis(20));
                    finished.fetch_add(1, Ordering::Relaxed);
                })
                .unwrap();
        }
        let joined: Vec<_> = words
            .iter()
            .map(|word| scope.spawn("telar-scoped", move || word.len()).unwrap())
            .collect();
        joined
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(lengths, [3, 3, 5]);
    assert_eq!(
        finished.load(Ordering::Relaxed),
        4,
        "every thread ended before the scope returned"
    );
}

#[test]
fn a_scope_waits_for_its_threads_when_its_body_unwinds() {
    let finished = AtomicUsize::new(0);
    let unwound = catch_unwind(AssertUnwindSafe(|| {
        scope(|scope| {
            scope
                .spawn("telar-scoped", || {
                    std::thread::sleep(Duration::from_millis(20));
                    finished.fetch_add(1, Ordering::Relaxed);
                })
                .unwrap();
            panic!("the body gave up");
        })
    }));
    assert!(unwound.is_err());
    assert_eq!(finished.load(Ordering::Relaxed), 1);
}

struct CountingStd(AtomicUsize);

unsafe impl Spawner for CountingStd {
    fn spawn(&self, name: String, main: Box<dyn FnOnce() + Send>) -> io::Result<Box<dyn Joinable>> {
        self.0.fetch_add(1, Ordering::Relaxed);
        let thread = thread::Builder::new().name(name).spawn(main)?;
        Ok(Box::new(StdThread(thread)))
    }
}

/// Installed for the whole test binary, which only counts: it starts each thread as std would.
static COUNTING: CountingStd = CountingStd(AtomicUsize::new(0));

#[test]
fn an_installed_spawner_starts_every_thread() {
    set_spawner(&COUNTING);
    let before = COUNTING.0.load(Ordering::Relaxed);
    assert_eq!(spawn("telar-test", || 1).unwrap().join().ok(), Some(1));
    let scoped = scope(|scope| scope.spawn("telar-scoped", || 2).unwrap().join().ok());
    assert_eq!(scoped, Some(2));
    assert!(COUNTING.0.load(Ordering::Relaxed) >= before + 2);
}
