use std::cell::Cell;
use std::rc::Rc;

use crate::{
    effect, live_effect_count, memo, owner_scope, reads_state, reset_runtime, signal, untrack,
};

#[test]
fn a_value_worked_out_from_nothing_read_nothing() {
    reset_runtime();
    assert_eq!(reads_state(|| 2 + 2), (4, false));
}

#[test]
fn a_signal_or_a_memo_read_on_the_way_is_state_read() {
    reset_runtime();
    let count = signal(3);
    let doubled = memo(move || count.get() * 2);
    assert_eq!(reads_state(move || count.get()), (3, true));
    assert_eq!(reads_state(move || doubled.get()), (6, true));
}

/// A read hidden behind a method, which nothing in the source of the call shows.
#[test]
fn a_read_behind_a_type_is_seen() {
    reset_runtime();
    struct Target(Rc<dyn Fn() -> bool>);
    impl Target {
        fn leaves(&self) -> bool {
            (self.0)()
        }
    }
    let external = signal(false);
    let target = Target(Rc::new(move || external.get()));
    assert_eq!(reads_state(|| target.leaves()), (false, true));
}

#[test]
fn a_read_inside_untrack_is_not_state_read() {
    reset_runtime();
    let count = signal(1);
    assert_eq!(reads_state(|| untrack(|| count.get())), (1, false));
}

#[test]
fn a_value_worked_out_from_one_that_read_state_read_it_too() {
    reset_runtime();
    let count = signal(1);
    let (value, read) = reads_state(|| reads_state(|| count.get()).0 + 1);
    assert_eq!((value, read), (2, true));
}

#[test]
fn reading_state_to_answer_does_not_subscribe_the_effect_around_it() {
    reset_runtime();
    let count = signal(0);
    let runs = Rc::new(Cell::new(0));
    let counted = Rc::clone(&runs);
    effect(move || {
        counted.set(counted.get() + 1);
        let _ = reads_state(|| count.get());
    });
    count.set(1);
    assert_eq!(runs.get(), 1, "only its first run");
}

#[test]
fn reading_inside_untrack_does_not_subscribe_the_effect_around_it() {
    reset_runtime();
    let count = signal(0);
    let seen = Rc::new(Cell::new(0));
    let last = Rc::clone(&seen);
    let other = signal(0);
    effect(move || {
        other.get();
        last.set(untrack(|| count.get()));
    });
    count.set(5);
    assert_eq!(seen.get(), 0, "not re-run by what it read once");
    other.set(1);
    assert_eq!(
        seen.get(),
        5,
        "and reads it fresh when something it follows runs it"
    );
}

#[test]
fn an_effect_that_read_nothing_is_let_go_after_its_run() {
    reset_runtime();
    let before = live_effect_count();
    let ran = Rc::new(Cell::new(false));
    let marked = Rc::clone(&ran);
    effect(move || marked.set(true));
    assert!(ran.get(), "it still runs");
    assert_eq!(live_effect_count(), before);
}

#[test]
fn an_effect_that_read_state_is_kept_and_follows_it() {
    reset_runtime();
    let before = live_effect_count();
    let count = signal(0);
    let seen = Rc::new(Cell::new(0));
    let last = Rc::clone(&seen);
    effect(move || last.set(count.get()));
    assert_eq!(live_effect_count(), before + 1);
    count.set(4);
    assert_eq!(seen.get(), 4);
}

#[test]
fn disposing_the_owner_of_an_effect_let_go_frees_what_is_left() {
    reset_runtime();
    let before = live_effect_count();
    let count = signal(0);
    let scope = {
        let guard = owner_scope();
        effect(|| {});
        effect(move || {
            count.get();
        });
        effect(|| {});
        guard.id()
    };
    assert_eq!(live_effect_count(), before + 1);
    crate::dispose_owner(scope);
    assert_eq!(live_effect_count(), before);
}

/// What an effect captured may be a registration whose drop undoes it, so letting the effect go must not let that go before its owner.
#[test]
fn what_an_effect_let_go_captured_lives_as_long_as_its_owner() {
    reset_runtime();
    struct Flag(Rc<Cell<bool>>);
    impl Drop for Flag {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    let dropped = Rc::new(Cell::new(false));
    let scope = {
        let guard = owner_scope();
        let held = Flag(Rc::clone(&dropped));
        effect(move || {
            let _ = &held;
        });
        guard.id()
    };
    assert!(!dropped.get(), "kept while its owner lives");
    crate::dispose_owner(scope);
    assert!(dropped.get(), "and freed with it");
}
