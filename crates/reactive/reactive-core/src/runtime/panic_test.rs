use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use super::*;
use crate::{Transaction, batch, effect, memo, signal};

fn quietly<R>(f: impl FnOnce() -> R) -> std::thread::Result<R> {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let outcome = catch_unwind(AssertUnwindSafe(f));
    std::panic::set_hook(hook);
    outcome
}

fn assert_settled() {
    RUNTIME.with(|rt| {
        let rt = rt.borrow();
        assert!(rt.observer_stack.is_empty(), "no effect is left running");
        assert!(rt.owner_stack.is_empty(), "no owner is left open");
        assert_eq!(rt.batch_depth, 0, "no batch is left open");
        assert_eq!(rt.disposing, 0, "no teardown is left in flight");
        assert!(!rt.flushing, "no flush is left in flight");
    });
}

type Caught = Rc<RefCell<Vec<String>>>;

fn message(payload: &PanicPayload) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_default()
}

fn catching(owner: OwnerId, caught: &Caught) {
    let caught = Rc::clone(caught);
    set_panic_catch(
        owner,
        Some(PanicCatch::Handle(Rc::new(move |payload| {
            caught.borrow_mut().push(message(&payload));
            Ok(())
        }))),
    );
}

#[test]
fn a_panicking_rerun_goes_to_the_nearest_catch_and_the_flush_carries_on() {
    let ticks = signal(0i32);
    let caught = Caught::default();
    let after = Rc::new(Cell::new(0));

    let scope = owner_scope();
    catching(scope.id(), &caught);
    effect(move || {
        if ticks.get() == 3 {
            panic!("third");
        }
    });
    drop(scope);
    let after_c = Rc::clone(&after);
    effect(move || after_c.set(ticks.get()));

    for n in 1..=4 {
        ticks.set(n);
    }

    assert_eq!(*caught.borrow(), vec!["third".to_string()]);
    assert_eq!(after.get(), 4, "the effect after it kept running");
    assert_settled();
}

#[test]
fn an_owner_marked_unwind_leaves_the_panic_to_its_build() {
    let caught = Caught::default();
    let outer = owner_scope();
    catching(outer.id(), &caught);
    let outcome = quietly(|| {
        let building = owner_scope();
        set_panic_catch(building.id(), Some(PanicCatch::Unwind));
        effect(|| panic!("first run"));
    });
    drop(outer);

    assert!(outcome.is_err(), "the build saw the panic");
    assert!(caught.borrow().is_empty(), "the outer catch never did");
    assert_settled();
}

#[test]
fn the_innermost_catch_wins_and_a_refusal_goes_up() {
    let ticks = signal(0i32);
    let outer_caught = Caught::default();
    let inner_calls = Rc::new(Cell::new(0));

    let outer = owner_scope();
    catching(outer.id(), &outer_caught);
    let inner = owner_scope();
    let calls = Rc::clone(&inner_calls);
    set_panic_catch(
        inner.id(),
        Some(PanicCatch::Handle(Rc::new(move |payload| {
            calls.set(calls.get() + 1);
            if calls.get() == 1 {
                Ok(())
            } else {
                Err(payload)
            }
        }))),
    );
    effect(move || {
        if ticks.get() > 0 {
            panic!("again");
        }
    });
    drop(inner);
    drop(outer);

    ticks.set(1);
    assert_eq!(inner_calls.get(), 1);
    assert!(outer_caught.borrow().is_empty(), "the inner catch took it");

    ticks.set(2);
    assert_eq!(inner_calls.get(), 2);
    assert_eq!(*outer_caught.borrow(), vec!["again".to_string()]);
    assert_settled();
}

/// A handler that disposes its scope must never run while an effect of that scope is still executing: that frees the closure mid-call.
#[test]
fn a_panic_inside_a_nested_run_is_offered_from_the_outer_run() {
    let ticks = signal(0i32);
    let disposed_by = Rc::new(Cell::new(0));

    let scope = owner_scope();
    let id = scope.id();
    let by = Rc::clone(&disposed_by);
    set_panic_catch(
        id,
        Some(PanicCatch::Handle(Rc::new(move |_| {
            by.set(by.get() + 1);
            dispose_owner(id);
            Ok(())
        }))),
    );
    let outer_runs = Rc::new(Cell::new(0));
    let runs = Rc::clone(&outer_runs);
    effect(move || {
        runs.set(runs.get() + 1);
        if ticks.get() == 1 {
            effect(|| panic!("nested"));
        }
    });
    drop(scope);
    let effects_before = live_effect_count();

    ticks.set(1);
    assert_eq!(disposed_by.get(), 1);
    assert_eq!(live_effect_count(), effects_before - 1, "the scope is gone");

    ticks.set(2);
    assert_eq!(outer_runs.get(), 2, "the disposed effect never ran again");
    assert_settled();
}

#[test]
fn a_panic_nothing_catches_leaves_the_rest_of_the_drain_queued() {
    let ticks = signal(0i32);
    effect(move || {
        if ticks.get() == 1 {
            panic!("uncaught");
        }
    });
    let seen = Rc::new(Cell::new(0));
    let seen_c = Rc::clone(&seen);
    effect(move || seen_c.set(ticks.get()));

    let outcome = quietly(|| ticks.set(1));
    assert!(outcome.is_err(), "nothing caught it, so the writer sees it");
    assert_settled();

    batch(|| {});
    assert_eq!(seen.get(), 1, "the effect the panic skipped ran next flush");
}

#[test]
fn a_panicking_cleanup_does_not_cut_a_teardown_short() {
    let ticks = signal(0i32);
    let ran = Rc::new(Cell::new(false));

    let scope = owner_scope();
    let held = signal(1i32);
    effect(move || {
        let _ = ticks.get();
    });
    on_cleanup(|| panic!("cleanup"));
    let ran_c = Rc::clone(&ran);
    on_cleanup(move || ran_c.set(true));
    let id = scope.id();
    drop(scope);
    let effects_before = live_effect_count();

    let outcome = quietly(|| dispose_owner(id));

    assert!(outcome.is_err(), "the panic is still reported");
    assert!(ran.get(), "the cleanup after it ran");
    assert!(!held.is_alive(), "the signal was freed");
    assert_eq!(
        live_effect_count(),
        effects_before - 1,
        "the effect was freed"
    );
    assert_settled();
}

/// A commit listener runs untracked, so the nested-run check must not read the (empty) observer stack, or a panic from an effect it created reaches a catch that disposes the scope of the effect still running around it.
#[test]
fn a_panic_under_untracked_inside_a_run_is_offered_from_the_outer_run() {
    let ticks = signal(0i32);
    let value = signal(0i32);
    let caught = Caught::default();
    let running_when_caught = Rc::new(Cell::new(None));

    let scope = owner_scope();
    let id = scope.id();
    let seen = Rc::clone(&caught);
    let running = Rc::clone(&running_when_caught);
    set_panic_catch(
        id,
        Some(PanicCatch::Handle(Rc::new(move |payload| {
            running.set(Some(RUNTIME.with(|rt| rt.borrow().running_effects)));
            seen.borrow_mut().push(message(&payload));
            dispose_owner(id);
            Ok(())
        }))),
    );
    let tx = Transaction::new(value).on_commit(|_, _| {
        effect(|| panic!("from the listener"));
    });
    let outer_runs = Rc::new(Cell::new(0));
    let runs = Rc::clone(&outer_runs);
    effect(move || {
        runs.set(runs.get() + 1);
        if ticks.get() == 1 {
            tx.begin().unwrap();
            tx.preview(|v| *v = 1).unwrap();
            let _ = tx.commit();
        }
    });
    drop(scope);

    ticks.set(1);
    assert_eq!(*caught.borrow(), ["from the listener"]);
    assert_eq!(
        running_when_caught.get(),
        Some(0),
        "offered once no run inside the scope was executing"
    );

    ticks.set(2);
    assert_eq!(outer_runs.get(), 2, "the disposed effect never ran again");
    assert_settled();
}

#[test]
fn a_memo_whose_computation_panicked_computes_again_when_read() {
    let source = signal(0i32);
    let failing = Rc::new(Cell::new(false));
    let fails = Rc::clone(&failing);
    let doubled = memo(move || {
        let v = source.get();
        if fails.get() {
            panic!("computation");
        }
        v * 2
    });

    failing.set(true);
    let outcome = quietly(|| source.set(1));
    assert!(outcome.is_err(), "nothing caught it, so the writer sees it");

    let read = quietly(|| doubled.get()).expect_err("still failing");
    assert_eq!(message(&read), "computation", "the real cause, not a cycle");

    failing.set(false);
    assert_eq!(doubled.get(), 2, "recomputed on read, with no write since");
    assert_settled();
}
