use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use crate::{after_settle, batch, effect, signal};

fn quietly<R>(f: impl FnOnce() -> R) -> std::thread::Result<R> {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let outcome = catch_unwind(AssertUnwindSafe(f));
    std::panic::set_hook(hook);
    outcome
}

#[test]
fn outside_a_batch_it_runs_at_once() {
    let ran = Rc::new(Cell::new(false));
    let seen = ran.clone();
    after_settle(move || seen.set(true));
    assert!(ran.get());
}

#[test]
fn inside_a_batch_it_runs_after_the_effects_the_batch_scheduled() {
    let source = signal(0);
    let derived = Rc::new(Cell::new(0));
    let written = derived.clone();
    effect(move || written.set(source.get()));
    let observed = Rc::new(Cell::new(None));
    let seen = observed.clone();
    let read = derived.clone();
    batch(|| {
        source.set(7);
        after_settle(move || seen.set(Some(read.get())));
        assert_eq!(observed.get(), None, "held until the batch closes");
    });
    assert_eq!(
        observed.get(),
        Some(7),
        "and run once the effect had seen the write"
    );
}

#[test]
fn a_batch_inside_an_effect_does_not_settle_the_flush_running_it() {
    let log = Rc::new(RefCell::new(Vec::<String>::new()));
    let trigger = signal(0u32);
    let a = signal(0u32);
    let b = signal(0u32);
    let writer_log = log.clone();
    effect(move || {
        let t = trigger.get();
        if t == 0 {
            return;
        }
        let settled = writer_log.clone();
        after_settle(move || settled.borrow_mut().push("settle".into()));
        a.set(t);
        batch(|| {});
        writer_log.borrow_mut().push("writer done".into());
        b.set(t);
    });
    let a_log = log.clone();
    effect(move || {
        let v = a.get();
        if v > 0 {
            a_log.borrow_mut().push(format!("a reader {v}"));
        }
    });
    let b_log = log.clone();
    effect(move || {
        let v = b.get();
        if v > 0 {
            b_log.borrow_mut().push(format!("b reader {v}"));
        }
    });

    trigger.set(1);

    assert_eq!(
        *log.borrow(),
        ["writer done", "a reader 1", "b reader 1", "settle"]
    );
}

#[test]
fn a_panicking_settle_callback_leaves_the_rest_queued() {
    let ran = Rc::new(RefCell::new(Vec::new()));
    let outcome = quietly(|| {
        batch(|| {
            let first = ran.clone();
            after_settle(move || first.borrow_mut().push(1));
            after_settle(|| panic!("settle"));
            let third = ran.clone();
            after_settle(move || third.borrow_mut().push(3));
        })
    });
    assert!(outcome.is_err(), "the panic reaches whoever settled");
    assert_eq!(*ran.borrow(), [1]);

    batch(|| {});
    assert_eq!(
        *ran.borrow(),
        [1, 3],
        "the next settle point ran what was left"
    );
}
