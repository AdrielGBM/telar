use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;
use web_time::Instant;

use super::*;
use crate::runtime::{set_current_surface, set_surface_enter_hook};

// `TASK_WAKER` and the pool are process-global while the registries are per-thread, so every test here serialises on this.
static SERIAL: Mutex<()> = Mutex::new(());

pub(in crate::task) fn serial() -> std::sync::MutexGuard<'static, ()> {
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
    set_surface_enter_hook(|handle, run| {
        let prev = set_current_surface(handle);
        run();
        set_current_surface(prev);
    });

    let seen = Rc::new(std::cell::Cell::new(SurfaceHandle::NONE));
    let sink = Rc::clone(&seen);
    SurfaceHandle::from_raw(7)
        .with_entered(|| spawn_task(|| (), move |()| sink.set(current_surface())));

    SurfaceHandle::from_raw(9).with_entered(|| drain_until(|| !seen.get().is_none()));
    assert_eq!(seen.get(), SurfaceHandle::from_raw(7));
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

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn the_pool_reuses_an_idle_thread() {
    let _serial = serial();
    spawn_task(|| (), |()| {});
    drain_until(|| pending_task_count() == 0);
    drain_until(|| super::pool::idle_and_spawned().0 > 0);
    let (_, spawned_before) = super::pool::idle_and_spawned();

    spawn_task(|| (), |()| {});
    drain_until(|| pending_task_count() == 0);
    assert!(
        super::pool::idle_and_spawned().1 <= spawned_before,
        "a task submitted while a worker is idle should run on it rather than on a new thread"
    );
}

struct DropFlag(Rc<std::cell::Cell<bool>>);

impl Drop for DropFlag {
    fn drop(&mut self) {
        self.0.set(true);
    }
}

/// A task's callback is code of the build that spawned it, so releasing the thread before that build is unloaded drops it, and a value its worker posts afterwards has nothing to run.
#[test]
fn releasing_a_thread_drops_the_callbacks_of_its_pending_tasks() {
    let _serial = serial();
    std::thread::spawn(|| {
        let dropped = Rc::new(std::cell::Cell::new(false));
        let flag = DropFlag(dropped.clone());
        let task = spawn_task(
            || std::thread::sleep(Duration::from_millis(20)),
            move |()| drop(flag),
        );
        assert!(task.is_pending());
        crate::thread_exit::release_thread();
        assert!(dropped.get());
        assert!(!task.is_pending());
        std::thread::sleep(Duration::from_millis(40));
        drain_tasks();
    })
    .join()
    .unwrap();
}

/// A surface's tasks are cancelled with it, but one that is torn down some other way still has no world to run them in, and the active one is another surface's.
#[test]
fn a_disposed_surfaces_task_never_runs() {
    let _serial = serial();
    let ran = Rc::new(std::cell::Cell::new(false));
    let sink = Rc::clone(&ran);
    let previous = set_current_surface(SurfaceHandle::from_raw(11));
    let task = spawn_task(|| (), move |()| sink.set(true));
    set_current_surface(previous);

    crate::dispose_surface(SurfaceHandle::from_raw(11));
    drain_until(|| !task.is_pending());
    assert!(!ran.get(), "its callback was dropped, not run");
}

/// A stream's entry is out of the registry while its item runs, so a surface torn down from inside that item cannot be cancelled there, and putting the entry back would keep delivering into a world that is gone.
#[test]
fn a_stream_whose_surface_goes_away_inside_its_item_is_stopped() {
    let _serial = serial();
    let surface = SurfaceHandle::from_raw(12);
    let seen = Rc::new(std::cell::Cell::new(0u32));
    let sink = Rc::clone(&seen);
    let (resume_tx, resume_rx) = std::sync::mpsc::channel();
    let (stopped_tx, stopped_rx) = std::sync::mpsc::channel();

    let previous = set_current_surface(surface);
    let task = spawn_stream(
        move |out| {
            out.emit(1u32);
            resume_rx.recv().ok();
            stopped_tx.send(out.is_cancelled()).ok();
            out.emit(2u32);
        },
        move |i| {
            sink.set(sink.get() + i);
            crate::dispose_surface(surface);
        },
        || unreachable!("on_end must not run for a stream whose surface is gone"),
    );
    set_current_surface(previous);

    drain_until(|| seen.get() != 0);
    assert!(
        !task.is_pending(),
        "the stream is released with its surface"
    );
    resume_tx.send(()).ok();
    assert_eq!(
        stopped_rx.recv_timeout(Duration::from_secs(5)),
        Ok(true),
        "its worker is told to stop"
    );
    std::thread::sleep(Duration::from_millis(20));
    drain_tasks();
    assert_eq!(
        seen.get(),
        1,
        "nothing is delivered after its surface went away"
    );
}
