use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use super::*;
use crate::{RwSignal, batch, effect, owner_scope, signal};

fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_owned()))
        .unwrap_or_default()
}

/// A value whose `Clone` writes the memo's own source, which flushes and recomputes the memo while `get` still lends it the value.
struct WritesOnClone {
    seen: i32,
    source: RwSignal<i32>,
}

impl PartialEq for WritesOnClone {
    fn eq(&self, other: &Self) -> bool {
        self.seen == other.seen
    }
}

impl Clone for WritesOnClone {
    fn clone(&self) -> Self {
        self.source.set(self.seen + 1);
        WritesOnClone {
            seen: self.seen,
            source: self.source,
        }
    }
}

#[test]
fn a_write_inside_with_recomputes_the_memo_after_the_closure_returns() {
    let scope = owner_scope();
    let source = signal(1i32);
    let doubled = memo(move || source.get() * 2);

    let lent = doubled.with(|value| {
        source.set(5);
        *value
    });
    assert_eq!(lent, 2, "the closure kept the value it was lent");
    assert_eq!(doubled.get(), 10);
    drop(scope);
}

#[test]
fn a_recompute_inside_a_read_panics_naming_the_memo_and_recomputes_afterwards() {
    let scope = owner_scope();
    let source = signal(0i32);
    let derived = memo(move || WritesOnClone {
        seen: source.get(),
        source,
    });
    let observed = Rc::new(Cell::new(-1));
    let into = Rc::clone(&observed);
    effect(move || into.set(derived.with(|value| value.seen)));
    assert_eq!(observed.get(), 0);

    let read_line = line!() + 1;
    let collided = catch_unwind(AssertUnwindSafe(|| derived.get().seen)).unwrap_err();
    let message = panic_message(&collided);
    assert!(
        message.contains("memo of type telar_reactive_core::memo::tests::WritesOnClone recomputed inside one of its own reads"),
        "{message}"
    );
    assert!(
        message.contains(&format!("read at: {}:{read_line}:", file!())),
        "the read it collided with is the caller's: {message}"
    );

    let unrelated = signal(0u8);
    effect(move || {
        unrelated.get();
    });
    unrelated.set(1);
    assert_eq!(
        observed.get(),
        1,
        "the next flush ran the recompute the collision missed, and its subscriber heard"
    );
    assert_eq!(derived.with(|value| value.seen), 1);
    drop(scope);
}

#[test]
fn a_memo_whose_recompute_was_missed_catches_up_on_its_next_read() {
    let scope = owner_scope();
    let source = signal(0i32);
    let derived = memo(move || WritesOnClone {
        seen: source.get(),
        source,
    });

    catch_unwind(AssertUnwindSafe(|| derived.get().seen)).unwrap_err();
    assert_eq!(
        derived.with(|value| value.seen),
        1,
        "the read recomputed what the collision left stale"
    );
    drop(scope);
}

fn logged<T: Clone + 'static>(log: &Rc<RefCell<Vec<T>>>, value: T) -> T {
    log.borrow_mut().push(value.clone());
    value
}

#[test]
fn a_diamond_reading_the_source_first_sees_the_memo_it_also_reads_settle() {
    let scope = owner_scope();
    let source = signal(1i32);
    let tens = memo(move || source.get() * 10);
    let computed = Rc::new(RefCell::new(Vec::new()));
    let log = Rc::clone(&computed);
    let sum = memo(move || logged(&log, source.get() + tens.get()));

    source.set(2);
    assert_eq!(sum.get(), 22);
    assert_eq!(*computed.borrow(), [11, 22], "no run saw the stale memo");
    drop(scope);
}

#[test]
fn a_diamond_reading_the_memo_first_sees_it_settle() {
    let scope = owner_scope();
    let source = signal(1i32);
    let tens = memo(move || source.get() * 10);
    let computed = Rc::new(RefCell::new(Vec::new()));
    let log = Rc::clone(&computed);
    let sum = memo(move || logged(&log, tens.get() + source.get()));

    source.set(2);
    assert_eq!(sum.get(), 22);
    assert_eq!(*computed.borrow(), [11, 22], "no run saw the stale memo");
    drop(scope);
}

#[test]
fn an_effect_below_a_diamond_observes_only_settled_values() {
    let scope = owner_scope();
    let source = signal(1i32);
    let tens = memo(move || source.get() * 10);
    let sum = memo(move || source.get() + tens.get());
    let seen = Rc::new(RefCell::new(Vec::new()));
    let log = Rc::clone(&seen);
    effect(move || {
        log.borrow_mut().push((source.get(), tens.get(), sum.get()));
    });

    source.set(2);
    source.set(3);
    assert_eq!(*seen.borrow(), [(1, 10, 11), (2, 20, 22), (3, 30, 33)]);
    drop(scope);
}

#[test]
fn an_effect_that_stops_reading_a_memo_stops_hearing_it() {
    let scope = owner_scope();
    let gate = signal(true);
    let source = signal(0i32);
    let doubled = memo(move || source.get() * 2);
    let runs = Rc::new(Cell::new(0));
    let counted = Rc::clone(&runs);
    effect(move || {
        counted.set(counted.get() + 1);
        if gate.get() {
            doubled.get();
        }
    });

    gate.set(false);
    assert_eq!(runs.get(), 2);
    for n in 1..=5 {
        source.set(n);
    }
    assert_eq!(runs.get(), 2, "the memo no longer read wakes nothing");

    gate.set(true);
    source.set(10);
    assert_eq!(runs.get(), 4, "read again, it is heard again");
    drop(scope);
}

#[test]
fn a_memo_at_the_end_of_a_chain_and_beside_its_root_computes_once_per_change() {
    let scope = owner_scope();
    let source = signal(1i32);
    let side = signal(0i32);
    let first = memo(move || source.get() + 1);
    let second = memo(move || first.get() * 2);
    let third = memo(move || second.get() + side.get());
    let computed = Rc::new(RefCell::new(Vec::new()));
    let log = Rc::clone(&computed);
    let last = memo(move || logged(&log, source.get() * 100 + third.get()));
    let seen = Rc::new(RefCell::new(Vec::new()));
    let observed = Rc::clone(&seen);
    effect(move || observed.borrow_mut().push(last.get()));

    source.set(2);
    side.set(5);
    batch(|| {
        source.set(3);
        side.set(7);
    });

    let settled = [104, 206, 211, 315];
    assert_eq!(*computed.borrow(), settled);
    assert_eq!(*seen.borrow(), settled);
    drop(scope);
}

#[test]
fn a_memo_that_starts_reading_a_deeper_one_lifts_its_readers_above_it() {
    let scope = owner_scope();
    let deep = signal(false);
    let source = signal(1i32);
    let first = memo(move || source.get());
    let second = memo(move || first.get());
    let third = memo(move || second.get());
    let picked = memo(move || {
        if deep.get() {
            third.get()
        } else {
            source.get()
        }
    });
    let computed = Rc::new(RefCell::new(Vec::new()));
    let log = Rc::clone(&computed);
    let sum = memo(move || logged(&log, source.get() + picked.get()));

    deep.set(true);
    source.set(2);
    assert_eq!(sum.get(), 4);
    assert_eq!(
        *computed.borrow(),
        [2, 4],
        "the reader ran after the chain it now sits on"
    );
    drop(scope);
}

#[test]
fn a_reader_still_pending_when_its_memo_deepens_waits_for_the_new_depth() {
    let scope = owner_scope();
    let deep = signal(false);
    let source = signal(1i32);
    let first = memo(move || source.get());
    let second = memo(move || first.get());
    let third = memo(move || second.get());
    let picked = memo(move || {
        if deep.get() {
            third.get()
        } else {
            source.get()
        }
    });
    let computed = Rc::new(RefCell::new(Vec::new()));
    let log = Rc::clone(&computed);
    let sum = memo(move || logged(&log, source.get() + picked.get()));

    batch(|| {
        deep.set(true);
        source.set(2);
    });
    assert_eq!(sum.get(), 4);
    assert_eq!(*computed.borrow(), [2, 4]);
    drop(scope);
}
