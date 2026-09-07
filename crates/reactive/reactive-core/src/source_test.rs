use super::*;
use crate::{reset_runtime, signal};

#[test]
fn a_derived_value_follows_its_source() {
    reset_runtime();
    let source = signal(2i32);
    let doubled = derive(source, |n| n * 2);
    assert_eq!(doubled.get(), 4, "seeded from the source, not a default");
    source.set(5);
    assert_eq!(doubled.get(), 10);
}

#[test]
fn a_pair_recomputes_when_either_half_moves() {
    reset_runtime();
    let level = signal(10i32);
    let charging = signal(false);
    let label = derive_pair(
        level.read_only(),
        charging.read_only(),
        |level, charging| format!("{level}{}", if charging { "+" } else { "" }),
    );
    assert_eq!(label.get(), "10");
    charging.set(true);
    assert_eq!(label.get(), "10+");
    level.set(11);
    assert_eq!(label.get(), "11+");
}

/// The regression this exists for. Deriving through a signal written by an effect seeds correctly and then goes dead the moment the handle drops.
#[test]
fn a_derivation_outlives_the_call_that_made_it() {
    reset_runtime();
    let source = signal(1i32);
    let derived = derive(source, |n| n * 10);
    let read: Box<dyn Fn() -> i32> = Box::new(move || derived.get());
    source.set(7);
    assert_eq!(read(), 70, "whatever holds the derivation keeps it alive");
}

/// A constant is a source too, so widening a widget's parameter does not cost every caller a signal.
#[test]
fn a_plain_value_reads_as_itself() {
    assert_eq!(Source::read(&0.5f32), 0.5);
    assert!(Source::read(&true), "a plain value reads as itself");
}

/// A derivation is a source, which is what lets one feed a widget that used to insist on a signal.
#[test]
fn a_derivation_is_itself_a_source() {
    reset_runtime();
    let source = signal(3i32);
    let once = derive(source, |n| n + 1);
    let twice = derive(once, |n| n * 2);
    assert_eq!(twice.get(), 8);
    source.set(4);
    assert_eq!(twice.get(), 10);
}
