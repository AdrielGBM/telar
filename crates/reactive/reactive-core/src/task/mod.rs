//! `spawn_task` / `spawn_stream` — the supported bridge from a worker thread back into the single-threaded reactive world.
//!
//! Signals are `!Send` by design, so a background result cannot be written where it is produced. The shape that does work is always the same: run the work on a thread, send the **data** back, and let the UI thread write the signal. This module is that shape, once, in the framework: the spawn functions take `Send` work and a `!Send` callback, keep the callback on the calling (UI) thread, and [`drain_tasks`] runs it there once a value arrives — the runner calls that per frame.
//!
//! Two shapes, because background work comes in two: [`spawn_task`] for work that produces one result, and [`spawn_stream`] for a worker that emits many (a watcher, a scan reporting progress).
//!
//! A callback re-enters the [`SurfaceHandle`] that was active at spawn time, exactly as the effect flush does, so one that touches its surface's layout/overlay/focus world resolves against the right one even when another surface's frame is what drained it.

mod pool;

use std::any::Any;
use std::cell::{Cell, Ref, RefCell, RefMut};
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use rustc_hash::FxHashMap;

use crate::runtime::{SurfaceHandle, current_surface};

type TaskId = u64;
type Waker = Arc<dyn Fn() + Send + Sync>;

/// Set by the runner so a finishing worker can wake the UI loop. Absent in headless and test contexts, where the caller drives [`drain_tasks`] itself.
static TASK_WAKER: RwLock<Option<Waker>> = RwLock::new(None);

/// Installs the process-global "wake the UI loop" used after a task posts a value. The runner passes the same wake an app gets from `AppCtx::redraw_waker`.
pub fn set_task_waker(wake: impl Fn() + Send + Sync + 'static) {
    *TASK_WAKER.write().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(wake));
}

fn wake_loop() {
    let waker = TASK_WAKER.read().unwrap_or_else(|e| e.into_inner()).clone();
    if let Some(wake) = waker {
        wake();
    }
}

enum Message {
    Item(Box<dyn Any + Send>),
    /// The worker returned (or unwound). Releases the callback.
    End,
}

/// What the UI thread runs for a delivered value. The distinction is lifetime: a task's callback is consumed by its one value, a stream's outlives every item and is followed by a close.
enum Callback {
    Once(Box<dyn FnOnce(Box<dyn Any + Send>)>),
    Stream {
        item: Box<dyn FnMut(Box<dyn Any + Send>)>,
        end: Box<dyn FnOnce()>,
    },
}

/// Values waiting for the UI thread to pick them up. Shared with every worker this thread spawned, so a worker that outlives the UI thread's interest still has somewhere valid to post into.
#[derive(Default)]
struct Mailbox {
    // Workers only push and the drain only takes, so a poisoned lock carries no torn state and recovering keeps one panicking task from wedging every future one. Push order is delivery order.
    posted: Mutex<Vec<(TaskId, Message)>>,
}

impl Mailbox {
    fn post(&self, id: TaskId, message: Message) {
        self.posted
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((id, message));
    }
}

struct PendingTask {
    callback: Callback,
    surface: SurfaceHandle,
    cancelled: Arc<AtomicBool>,
}

#[derive(Default)]
struct TaskRegistry {
    next_id: TaskId,
    pending: FxHashMap<TaskId, PendingTask>,
    mailbox: Arc<Mailbox>,
}

// The same raw-pointer idiom as the reactive runtime cell: no `Drop`, so no TLS destructor is registered and dlclosing a hot-reload dylib on thread exit stays safe. The one allocation per thread is leaked.
struct TaskCell(Cell<*mut RefCell<TaskRegistry>>);

impl TaskCell {
    fn borrow(&self) -> Ref<'_, TaskRegistry> {
        unsafe { (*self.0.get()).borrow() }
    }
    fn borrow_mut(&self) -> RefMut<'_, TaskRegistry> {
        unsafe { (*self.0.get()).borrow_mut() }
    }
}

thread_local! {
    static TASKS: TaskCell = TaskCell(Cell::new(
        Box::into_raw(Box::new(RefCell::new(TaskRegistry::default())))
    ));
}

/// The worker's end of a task. Posting `End` from `Drop` is what releases the callback on *both* exits — a normal return and an unwind — so work that panics abandons its task instead of leaving it pending forever.
struct WorkerEnd {
    mailbox: Arc<Mailbox>,
    id: TaskId,
    cancelled: Arc<AtomicBool>,
}

impl WorkerEnd {
    fn post(&self, message: Message) {
        self.mailbox.post(self.id, message);
        wake_loop();
    }
}

impl Drop for WorkerEnd {
    fn drop(&mut self) {
        self.post(Message::End);
    }
}

/// A spawned task or stream. Dropping it detaches — the work keeps running and its callback keeps firing. Keep it to [`cancel`](Task::cancel) when whatever the callback would write is going away.
///
/// Deliberately `!Send`: the callback it controls lives in the spawning thread's registry, so a handle taken to another thread could only cancel that thread's tasks instead.
pub struct Task {
    id: TaskId,
    cancelled: Arc<AtomicBool>,
    _ui_thread_only: PhantomData<*const ()>,
}

impl Task {
    /// Drops the callback, so anything the worker still posts is discarded. The thread is not interrupted — `std::thread` has no way to do that — but a [`spawn_stream`] worker polling [`Emitter::is_cancelled`] can stop on its own.
    pub fn cancel(self) {
        self.cancelled.store(true, Ordering::Relaxed);
        let dropped = TASKS.with(|t| t.borrow_mut().pending.remove(&self.id));
        drop(dropped);
    }

    /// Whether the callback is still registered — the value has yet to arrive, or the stream is still open.
    pub fn is_pending(&self) -> bool {
        TASKS.with(|t| t.borrow().pending.contains_key(&self.id))
    }
}

fn register(surface: SurfaceHandle, callback: Callback) -> (TaskId, Arc<Mailbox>, Arc<AtomicBool>) {
    TASKS.with(|t| {
        let mut registry = t.borrow_mut();
        let id = registry.next_id;
        registry.next_id += 1;
        let cancelled = Arc::new(AtomicBool::new(false));
        registry.pending.insert(
            id,
            PendingTask {
                callback,
                surface,
                cancelled: Arc::clone(&cancelled),
            },
        );
        (id, Arc::clone(&registry.mailbox), cancelled)
    })
}

/// Runs `work` on a background thread and `on_done` with its result **on this thread**, during a later frame's [`drain_tasks`].
///
/// This is the supported way to get a background result into a signal: `on_done` stays here, so it may close over `!Send` state and write signals directly, while `work` and the value it produces cross the thread boundary and must be `Send`.
///
/// ```ignore
/// spawn_task(
///     || expensive_query(),          // worker thread
///     move |rows| results.set(rows), // UI thread, a later frame
/// );
/// ```
///
/// Work runs on a pooled thread that may block freely — the pool grows rather than starve. A panic inside `work` abandons the task: `on_done` is dropped without running.
pub fn spawn_task<T, W, F>(work: W, on_done: F) -> Task
where
    T: Send + 'static,
    W: FnOnce() -> T + Send + 'static,
    F: FnOnce(T) + 'static,
{
    let callback = Callback::Once(Box::new(move |value| {
        if let Ok(value) = value.downcast::<T>() {
            on_done(*value);
        }
    }));
    let (id, mailbox, cancelled) = register(current_surface(), callback);

    let end = WorkerEnd {
        mailbox,
        id,
        cancelled: Arc::clone(&cancelled),
    };
    pool::submit(Box::new(move || {
        let value = work();
        end.post(Message::Item(Box::new(value)));
    }));

    Task {
        id,
        cancelled,
        _ui_thread_only: PhantomData,
    }
}

/// The worker's handle to a [`spawn_stream`], for posting items back to the UI thread. `Send` and cloneable, so the work can hand it to nested helpers or a callback-driven library.
pub struct Emitter<T> {
    end: Arc<WorkerEnd>,
    _item: PhantomData<fn(T)>,
}

impl<T: Send + 'static> Emitter<T> {
    /// Posts one item and wakes the UI loop. A no-op once the stream is cancelled.
    pub fn emit(&self, item: T) {
        if self.is_cancelled() {
            return;
        }
        self.end.post(Message::Item(Box::new(item)));
    }

    /// Whether the [`Task`] was cancelled. A long-running worker should poll this and return — nothing else can stop it, and everything it emits from here on is discarded.
    pub fn is_cancelled(&self) -> bool {
        self.end.cancelled.load(Ordering::Relaxed)
    }
}

impl<T> Clone for Emitter<T> {
    fn clone(&self) -> Self {
        Self {
            end: Arc::clone(&self.end),
            _item: PhantomData,
        }
    }
}

/// Runs `work` on a background thread, handing it an [`Emitter`], and runs `on_item` **on this thread** for every item it emits — in order, during the frames that follow. `on_end` runs once the worker returns.
///
/// The [`spawn_task`] shape for work that produces many values rather than one: a file watcher, a scan reporting progress, a paged download.
///
/// ```ignore
/// spawn_stream(
///     |out| for path in walk_project() { out.emit(path); },
///     move |path| found.update(|list| list.push(path)),
///     move || scanning.set(false),
/// );
/// ```
///
/// `on_end` fires after the last item, whether the worker returned or unwound — a stream cannot report *why* it stopped, only that it did. Cancelling instead drops both callbacks, so `on_end` does **not** run: the caller already knows, and is usually the one tearing that state down.
///
/// Everything posted between two frames is run in the next one, so a worker that emits faster than the UI can absorb makes for long frames. Emit coarse progress, not one item per unit of work.
pub fn spawn_stream<T, W, F, E>(work: W, mut on_item: F, on_end: E) -> Task
where
    T: Send + 'static,
    W: FnOnce(Emitter<T>) + Send + 'static,
    F: FnMut(T) + 'static,
    E: FnOnce() + 'static,
{
    let callback = Callback::Stream {
        item: Box::new(move |value| {
            if let Ok(value) = value.downcast::<T>() {
                on_item(*value);
            }
        }),
        end: Box::new(on_end),
    };
    let (id, mailbox, cancelled) = register(current_surface(), callback);

    let emitter = Emitter::<T> {
        end: Arc::new(WorkerEnd {
            mailbox,
            id,
            cancelled: Arc::clone(&cancelled),
        }),
        _item: PhantomData,
    };
    pool::submit(Box::new(move || work(emitter)));

    Task {
        id,
        cancelled,
        _ui_thread_only: PhantomData,
    }
}

/// Runs the callbacks for every value posted since the last call. The runner calls this once per frame, on the UI thread, before `App::on_frame`.
///
/// Callbacks run inside a batch, so a frame's worth of deliveries costs one flush.
pub fn drain_tasks() {
    let posted = TASKS.with(|t| {
        let registry = t.borrow();
        let mut posted = registry
            .mailbox
            .posted
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        std::mem::take(&mut *posted)
    });
    if posted.is_empty() {
        return;
    }

    crate::batch(|| {
        for (id, message) in posted {
            // The entry is taken out of the registry while its callback runs, so the callback may spawn or cancel tasks, and a stream cannot be re-entered by a nested drain.
            let Some(task) = TASKS.with(|t| t.borrow_mut().pending.remove(&id)) else {
                continue; // Cancelled; the value goes with it.
            };
            let value = match message {
                Message::Item(value) => value,
                Message::End => {
                    if let Callback::Stream { end, .. } = task.callback {
                        let _surface = task.surface.enter();
                        end();
                    }
                    continue;
                }
            };
            let still_open = {
                let _surface = task.surface.enter();
                match task.callback {
                    Callback::Once(run) => {
                        run(value);
                        None
                    }
                    Callback::Stream { mut item, end } => {
                        item(value);
                        Some(Callback::Stream { item, end })
                    }
                }
            };
            if let Some(callback) = still_open {
                TASKS.with(|t| {
                    t.borrow_mut().pending.insert(
                        id,
                        PendingTask {
                            callback,
                            surface: task.surface,
                            cancelled: task.cancelled,
                        },
                    )
                });
            }
        }
    });
}

/// Cancels everything spawned while `surface` was the active one, discarding their callbacks and telling stream workers to stop. Use it when a surface goes away, so work started for it cannot write into the world it left.
pub fn cancel_tasks_for(surface: SurfaceHandle) {
    // Collected out of the registry before being dropped: a callback's captured state may spawn or cancel tasks from its own `Drop`, which would re-enter this borrow.
    let cancelled: Vec<PendingTask> = TASKS.with(|t| {
        let mut registry = t.borrow_mut();
        let ids: Vec<TaskId> = registry
            .pending
            .iter()
            .filter(|(_, task)| task.surface == surface)
            .map(|(id, _)| *id)
            .collect();
        ids.iter()
            .filter_map(|id| registry.pending.remove(id))
            .inspect(|task| task.cancelled.store(true, Ordering::Relaxed))
            .collect()
    });
    drop(cancelled);
}

/// Stops the worker pool and drops every pending callback. Called before a hot-reload dylib is closed: its threads are parked in, and its callbacks are made of, code that is about to be unmapped.
///
/// Joining the pool means a reload waits for in-flight background work — see `pool::shutdown_and_join`.
pub fn reset_tasks() {
    let flags: Vec<Arc<AtomicBool>> = TASKS.with(|t| {
        t.borrow()
            .pending
            .values()
            .map(|task| Arc::clone(&task.cancelled))
            .collect()
    });
    for flag in flags {
        flag.store(true, Ordering::Relaxed);
    }
    // Before the registry is cleared, so a worker still finishing has a live mailbox to post its `End` into.
    pool::shutdown_and_join();

    let (pending, mailbox) = TASKS.with(|t| {
        let mut registry = t.borrow_mut();
        (
            std::mem::take(&mut registry.pending),
            std::mem::take(&mut registry.mailbox),
        )
    });
    drop(pending);
    drop(mailbox);
}

/// How many spawned tasks and open streams still hold a callback. Test-only: nothing in a normal build asks.
#[cfg(test)]
fn pending_task_count() -> usize {
    TASKS.with(|t| t.borrow().pending.len())
}

#[cfg(test)]
mod tests {
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
        assert!(!ran.get());
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
        assert!(!ran.get());
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
        assert!(WAKES.load(Ordering::SeqCst) >= 1);
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
}
