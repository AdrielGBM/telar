use std::rc::Rc;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;
use web_time::Instant;

use super::*;
use crate::runtime::{SurfaceEnterGuard, set_current_surface, set_surface_enter_hook};

// `TASK_WAKER` and the pool are process-global while the registries are per-thread, so every test here serialises on this.
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

fn drain_until(done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(Instant::now() < deadline, "task never completed");
        std::thread::sleep(Duration::from_millis(1));
        drain_tasks();
    }
}

#[test]
fn result_crosses_back_to_the_spawning_thread() {
    let _serial = serial();
    let got = Rc::new(std::cell::Cell::new(0i32));
    let sink = Rc::clone(&got);
    spawn_task(|| 6 * 7, move |v| sink.set(v));
    drain_until(|| got.get() != 0);
    assert_eq!(got.get(), 42);
    assert_eq!(pending_task_count(), 0);
}

#[test]
fn cancel_discards_the_result() {
    let _serial = serial();
    let ran = Rc::new(std::cell::Cell::new(false));
    let sink = Rc::clone(&ran);
    let task = spawn_task(|| 1u8, move |_| sink.set(true));
    task.cancel();
    assert_eq!(pending_task_count(), 0);
    std::thread::sleep(Duration::from_millis(20));
    drain_tasks();
    assert!(!ran.get(), "a cancelled task must not run its callback");
}

#[test]
fn a_panicking_worker_abandons_its_callback() {
    let _serial = serial();
    let ran = Rc::new(std::cell::Cell::new(false));
    let sink = Rc::clone(&ran);
    spawn_task(
        || -> u8 { panic!("worker blew up") },
        move |_| sink.set(true),
    );
    drain_until(|| pending_task_count() == 0);
    assert!(
        !ran.get(),
        "a worker that panicked has no result to deliver"
    );
}

// Mirrors the effect flush: a callback resolves against the surface world that spawned it, whichever surface's frame happens to drain the queue.
#[test]
fn completion_reenters_the_spawning_surface() {
    let _serial = serial();
    set_surface_enter_hook(|handle| {
        let prev = set_current_surface(handle);
        SurfaceEnterGuard::new(move || {
            set_current_surface(prev);
        })
    });

    let seen = Rc::new(std::cell::Cell::new(SurfaceHandle::NONE));
    let sink = Rc::clone(&seen);
    {
        let _surface_a = SurfaceHandle(7).enter();
        spawn_task(|| (), move |()| sink.set(current_surface()));
    }

    let _surface_b = SurfaceHandle(9).enter();
    drain_until(|| !seen.get().is_none());
    assert_eq!(seen.get(), SurfaceHandle(7));
}

#[test]
fn a_finishing_worker_wakes_the_loop() {
    let _serial = serial();
    static WAKES: AtomicUsize = AtomicUsize::new(0);
    WAKES.store(0, Ordering::SeqCst);
    set_task_waker(|| {
        WAKES.fetch_add(1, Ordering::SeqCst);
    });

    spawn_task(|| (), |()| {});
    drain_until(|| pending_task_count() == 0);
    assert!(
        WAKES.load(Ordering::SeqCst) >= 1,
        "a finished worker has to wake the loop, or its result waits for an unrelated frame"
    );
}

#[test]
fn a_stream_delivers_every_item_in_order_then_closes() {
    let _serial = serial();
    let seen: Rc<RefCell<Vec<u32>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&seen);
    let closed = Rc::new(std::cell::Cell::new(false));
    let closed_sink = Rc::clone(&closed);
    spawn_stream(
        |out| {
            for i in 0..8u32 {
                out.emit(i);
            }
        },
        move |i| sink.borrow_mut().push(i),
        move || closed_sink.set(true),
    );
    drain_until(|| pending_task_count() == 0);
    assert_eq!(*seen.borrow(), (0..8).collect::<Vec<u32>>());
    assert!(closed.get(), "on_end must run once the worker returns");
}

#[test]
fn cancelling_a_stream_stops_its_worker() {
    let _serial = serial();
    let seen = Rc::new(std::cell::Cell::new(0u32));
    let sink = Rc::clone(&seen);
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    static EMITTED: AtomicUsize = AtomicUsize::new(0);
    EMITTED.store(0, Ordering::SeqCst);

    let task = spawn_stream(
        move |out| {
            started_tx.send(()).ok();
            while !out.is_cancelled() {
                out.emit(1u32);
                EMITTED.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(1));
            }
        },
        move |i| sink.set(sink.get() + i),
        || unreachable!("on_end must not run for a cancelled stream"),
    );

    started_rx.recv().expect("worker never started");
    task.cancel();
    assert_eq!(
        pending_task_count(),
        0,
        "cancel releases the callback at once"
    );

    std::thread::sleep(Duration::from_millis(60));
    let settled = EMITTED.load(Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(60));
    assert_eq!(
        settled,
        EMITTED.load(Ordering::SeqCst),
        "cancel must stop the stream worker, not just mute it"
    );
    drain_tasks();
    assert_eq!(seen.get(), 0, "no item may be delivered after cancel");
}

#[test]
fn the_pool_reuses_an_idle_thread() {
    let _serial = serial();
    let threads: Rc<RefCell<Vec<std::thread::ThreadId>>> = Rc::new(RefCell::new(Vec::new()));
    for _ in 0..2 {
        let sink = Rc::clone(&threads);
        spawn_task(
            || std::thread::current().id(),
            move |id| sink.borrow_mut().push(id),
        );
        drain_until(|| pending_task_count() == 0);
    }
    let threads = threads.borrow();
    assert_eq!(threads.len(), 2);
    assert_eq!(
        threads[0], threads[1],
        "the second task should have reused the idle worker"
    );
}
