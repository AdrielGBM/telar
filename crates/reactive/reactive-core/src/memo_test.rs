use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use super::*;
use crate::{RwSignal, effect, owner_scope, signal};

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
