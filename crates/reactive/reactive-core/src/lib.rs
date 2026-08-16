//! Factory functions (`signal`, `effect`, `memo`) create
//! nodes in the reactive graph. Struct constructors (`Runtime::new`, etc.) own
//! their state. Free functions (`batch`, `set_flush_notify`) operate on the
//! thread-local runtime.

mod effect;
mod memo;
pub use reactive_local::reentry;
mod runtime;
mod signal;
#[macro_use]

mod task;

pub use effect::{Effect, effect};
pub use memo::{Memo, memo};
pub use reactive_local::{SurfaceSlot, surface_local};
pub use runtime::{
    FlushNotifyHandle, SurfaceEnterGuard, SurfaceHandle, batch, begin_batch, current_surface,
    end_batch, reset_runtime, set_current_surface, set_flush_notify, set_surface_enter_hook,
};
pub use signal::{ReadSignal, RwSignal, signal};
pub use task::{
    Emitter, Task, cancel_tasks_for, drain_tasks, reset_tasks, set_task_waker, spawn_stream,
    spawn_task,
};

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    #[test]
    fn signal_get_set() {
        let count = signal(0i32);
        assert_eq!(count.get(), 0);
        count.set(42);
        assert_eq!(count.get(), 42);
    }

    // M3 owner-scope: the shared runtime stamps each effect with the surface active at registration, and the
    // flush re-enters that surface before running the effect — even when the write that scheduled it happened
    // under a different active surface. Here the enter hook records which surface each run resolved against.
    #[test]
    fn effect_runs_under_its_own_surface_context() {
        use std::cell::RefCell;
        use std::rc::Rc;

        let entered: Rc<RefCell<Vec<u64>>> = Rc::new(RefCell::new(Vec::new()));
        let entered_hook = Rc::clone(&entered);
        set_surface_enter_hook(move |handle| {
            let prev = set_current_surface(handle);
            entered_hook.borrow_mut().push(handle.0);
            SurfaceEnterGuard::new(move || {
                set_current_surface(prev);
            })
        });

        // Build an effect "owned by" surface A: A is active while it registers, so it captures A.
        let trigger = signal(0i32);
        let read = trigger.read_only();
        let _guard_a = SurfaceHandle(1).enter();
        let _e = effect(move || {
            read.get();
        });
        drop(_guard_a);

        // Back on the ambient surface, no A entry has been recorded beyond the build itself; clear the log so
        // we observe only what the *flush-triggered* run enters.
        entered.borrow_mut().clear();

        // Write the signal while surface B is active. The scheduled effect belongs to A, so the flush must
        // enter A (1), not B (2), before running it.
        let _guard_b = SurfaceHandle(2).enter();
        trigger.set(1);
        drop(_guard_b);

        assert!(
            entered.borrow().contains(&1),
            "flush must re-enter the effect's own surface (A=1): {:?}",
            entered.borrow()
        );
    }

    // A panic inside `batch` must leave the shared runtime consistent (batch_depth/flushing reset), so a
    // later write still schedules and flushes effects. Without the RAII guards this would wedge the runtime.
    #[test]
    fn runtime_recovers_after_panic_in_batch() {
        use std::cell::RefCell;
        use std::panic::{AssertUnwindSafe, catch_unwind};
        use std::rc::Rc;

        let count = signal(0i32);
        let read = count.read_only();
        let seen: Rc<RefCell<Vec<i32>>> = Rc::new(RefCell::new(Vec::new()));
        let seen_c = Rc::clone(&seen);
        let _e = effect(move || {
            seen_c.borrow_mut().push(read.get());
        });

        let result = catch_unwind(AssertUnwindSafe(|| {
            batch(|| {
                count.set(1);
                panic!("boom");
            });
        }));
        assert!(result.is_err(), "the batch closure should have panicked");

        count.set(2);
        assert!(
            seen.borrow().contains(&2),
            "runtime wedged after panic-in-batch; effect never re-ran: {:?}",
            seen.borrow()
        );
    }

    // A cascade inside one flush: `writer` runs after `reader` and writes the signal `reader` depends on, so
    // `reader` has to run again. It is scheduled either way — the regression was the flush-wide epoch stamp,
    // which turned that second run into a no-op with nothing left to reschedule it, so the reader kept the
    // stale value until some later, unrelated write opened a fresh flush.
    #[test]
    fn an_effect_reruns_when_a_later_effect_in_the_same_flush_writes_its_source() {
        let trigger = signal(0i32);
        let source = signal(0i32);
        let seen: Rc<RefCell<Vec<i32>>> = Rc::new(RefCell::new(Vec::new()));

        // The reader is scheduled by the same write as the writer, and runs first — so by the time the writer
        // moves `source`, the reader has already run once in this flush.
        let read_trigger = trigger.read_only();
        let read_source = source.read_only();
        let seen_c = Rc::clone(&seen);
        let _reader = effect(move || {
            read_trigger.get();
            seen_c.borrow_mut().push(read_source.get());
        });

        let read_trigger = trigger.read_only();
        let write_source = source.clone();
        let _writer = effect(move || {
            let v = read_trigger.get();
            if v > 0 {
                write_source.set(v);
            }
        });

        seen.borrow_mut().clear();
        trigger.set(7);
        assert_eq!(
            seen.borrow().last().copied(),
            Some(7),
            "the reader never saw a write made later in the same flush: {:?}",
            seen.borrow()
        );
    }

    #[test]
    fn signal_update() {
        let count = signal(10i32);
        count.update(|v| *v *= 2);
        assert_eq!(count.get(), 20);
    }

    #[test]
    fn signal_with() {
        let name = signal(String::from("rsx"));
        let len = name.with(|s| s.len());
        assert_eq!(len, 3);
    }

    #[test]
    fn rw_signal() {
        let count = signal(0i32);
        count.set(10);
        assert_eq!(count.get(), 10);
        count.update(|v| *v += 5);
        assert_eq!(count.get(), 15);
    }

    #[test]
    fn rw_signal_read_only() {
        let sig = signal(0i32);
        let read = sig.read_only();
        sig.set(7);
        assert_eq!(read.get(), 7);
    }

    #[test]
    fn bool_signal_toggle() {
        let flag = signal(false);
        flag.toggle();
        assert!(flag.get());
        flag.toggle();
        assert!(!flag.get());
    }

    #[test]
    fn effect_runs_immediately() {
        let ran = Rc::new(RefCell::new(false));
        let ran_clone = Rc::clone(&ran);
        let _e = effect(move || {
            *ran_clone.borrow_mut() = true;
        });
        assert!(*ran.borrow());
    }

    #[test]
    fn effect_reruns_on_signal_change() {
        let count = signal(0i32);
        let log: Rc<RefCell<Vec<i32>>> = Rc::new(RefCell::new(Vec::new()));
        let log_clone = Rc::clone(&log);

        let read = count.read_only();
        let _e = effect(move || {
            log_clone.borrow_mut().push(read.get());
        });

        count.set(1);
        count.set(2);

        assert_eq!(*log.borrow(), vec![0, 1, 2]);
    }

    #[test]
    fn memo_derives_value() {
        let count = signal(2i32);
        let read = count.read_only();
        let doubled = memo(move || read.get() * 2);

        assert_eq!(doubled.get(), 4);
        count.set(5);
        assert_eq!(doubled.get(), 10);
    }

    #[test]
    fn effect_reruns_when_memo_changes() {
        let count = signal(0i32);
        let read = count.read_only();
        let doubled = memo(move || read.get() * 2);
        let log: Rc<RefCell<Vec<i32>>> = Rc::new(RefCell::new(Vec::new()));
        let log_clone = Rc::clone(&log);
        let doubled_read = doubled.clone();
        let _e = effect(move || {
            log_clone.borrow_mut().push(doubled_read.get());
        });
        assert_eq!(*log.borrow(), vec![0]);
        count.set(3);
        assert_eq!(*log.borrow(), vec![0, 6]);
    }

    // Regression: an effect tracking BOTH a signal and a memo must re-run when only the memo's source changes — run_effect's signal-version shortcut used to skip it because memo deps are invisible to `sources` (the sandbox counter's frozen "Double:" text).
    #[test]
    fn effect_with_signal_and_memo_sources_reruns_on_memo_change() {
        let unrelated = signal(0i32);
        let count = signal(0i32);
        let read = count.read_only();
        let doubled = memo(move || read.get() * 2);
        let log: Rc<RefCell<Vec<i32>>> = Rc::new(RefCell::new(Vec::new()));
        let log_clone = Rc::clone(&log);
        let unrelated_read = unrelated.read_only();
        let doubled_read = doubled.clone();
        let _e = effect(move || {
            unrelated_read.get();
            log_clone.borrow_mut().push(doubled_read.get());
        });
        assert_eq!(*log.borrow(), vec![0]);
        count.set(3);
        assert_eq!(*log.borrow(), vec![0, 6]);
    }

    #[test]
    fn effect_reruns_when_memo_changes_inside_batch() {
        let count = signal(0i32);
        let read = count.read_only();
        let doubled = memo(move || read.get() * 2);
        let log: Rc<RefCell<Vec<i32>>> = Rc::new(RefCell::new(Vec::new()));
        let log_clone = Rc::clone(&log);
        let doubled_read = doubled.clone();
        let _e = effect(move || {
            log_clone.borrow_mut().push(doubled_read.get());
        });
        batch(|| count.set(3));
        assert_eq!(*log.borrow(), vec![0, 6]);
    }

    #[test]
    fn memo_chains() {
        let n = signal(3i32);
        let read = n.read_only();
        let doubled = memo(move || read.get() * 2);
        let doubled_for_quad = doubled.clone();
        let quadrupled = memo(move || doubled_for_quad.get() * 2);

        assert_eq!(quadrupled.get(), 12);
        n.set(5);
        assert_eq!(quadrupled.get(), 20);
    }

    #[test]
    fn batch_fires_effect_once() {
        let a = signal(0i32);
        let b = signal(0i32);
        let runs = Rc::new(RefCell::new(0usize));
        let runs_clone = Rc::clone(&runs);

        let a_read = a.read_only();
        let b_read = b.read_only();
        let _e = effect(move || {
            let _ = a_read.get() + b_read.get();
            *runs_clone.borrow_mut() += 1;
        });

        assert_eq!(*runs.borrow(), 1);

        batch(|| {
            a.set(1);
            b.set(2);
        });

        assert_eq!(*runs.borrow(), 2);
    }

    #[test]
    fn dropping_one_subscriber_keeps_others_consistent() {
        // Three effects subscribe to the same signal. Dropping the middle one and then writing repeatedly must keep the survivors firing and must not panic — exercising the in-place subscriber-list cleanup that runs alongside notify_signal's reused scratch buffer (the cleanup must stay correct now that subscribers are no longer cloned per write).
        let count = signal(0i32);
        let a = Rc::new(RefCell::new(0i32));
        let b = Rc::new(RefCell::new(0i32));
        let c = Rc::new(RefCell::new(0i32));

        let mk = |sink: &Rc<RefCell<i32>>, sig: &RwSignal<i32>| {
            let read = sig.read_only();
            let sink = Rc::clone(sink);
            effect(move || {
                *sink.borrow_mut() = read.get();
            })
        };

        let _ea = mk(&a, &count);
        let eb = mk(&b, &count);
        let _ec = mk(&c, &count);

        count.set(1);
        assert_eq!((*a.borrow(), *b.borrow(), *c.borrow()), (1, 1, 1));

        drop(eb);
        count.set(2);
        count.set(3);

        // Survivors tracked every write; the dropped one is frozen at its last value, with no panic and no lost survivor.
        assert_eq!(*a.borrow(), 3);
        assert_eq!(*c.borrow(), 3);
        assert_eq!(*b.borrow(), 1);
    }

    // A signal whose value owns other signal handles must drop cleanly: dropping the outer signal removes its
    // storage and then drops the value, which re-enters `drop_signal` for the inner handles. If the value were
    // dropped while the runtime borrow was still held, that re-entry would abort during teardown.
    #[test]
    fn dropping_a_signal_whose_value_holds_signals_does_not_double_borrow() {
        let inner = signal(1i32);
        let outer = signal(vec![inner.clone()]);
        drop(inner);
        drop(outer);
        // Reaching here without aborting is the assertion; the runtime is still usable afterwards.
        let after = signal(5i32);
        assert_eq!(after.get(), 5);
    }
}
