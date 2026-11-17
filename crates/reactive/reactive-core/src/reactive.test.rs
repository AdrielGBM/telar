use super::*;
use crate::{memo, reset_runtime, signal};

#[test]
fn a_constant_reads_as_itself() {
    reset_runtime();
    let tint: Reactive<u32> = 7.into();
    assert_eq!(tint.get(), 7);
}

#[test]
fn a_signal_is_re_read_rather_than_frozen() {
    reset_runtime();
    let count = signal(1i32);
    let reading: Reactive<i32> = count.into();
    assert_eq!(reading.get(), 1);
    count.set(9);
    assert_eq!(reading.get(), 9, "the prop froze at construction");
}

/// The case a component declaring `RwSignal<T>` cannot serve, which is why this type exists.
#[test]
fn a_derivation_over_two_signals_is_a_reading_too() {
    reset_runtime();
    let (width, height) = (signal(3i32), signal(4i32));
    let area = memo(move || width.get() * height.get());
    let reading: Reactive<i32> = area.into();
    assert_eq!(reading.get(), 12);
    width.set(5);
    assert_eq!(reading.get(), 20);
}

#[test]
fn a_closure_needs_the_named_constructor() {
    reset_runtime();
    let count = signal(2i32);
    let doubled = Reactive::of(move || count.get() * 2);
    assert_eq!(doubled.get(), 4);
    count.set(10);
    assert_eq!(doubled.get(), 20);
}

#[test]
fn a_literal_reads_as_an_owned_string() {
    reset_runtime();
    let label: Reactive<String> = "Save".into();
    assert_eq!(label.get(), "Save");
}

#[test]
fn a_value_that_read_nothing_is_followed_as_a_constant() {
    reset_runtime();
    let reading = Reactive::<i32>::follow(|| 2 + 3);
    assert!(matches!(reading, Reactive::Const(5)));
}

#[test]
fn a_value_that_read_state_is_followed() {
    reset_runtime();
    let count = signal(1i32);
    let reading = Reactive::<i32>::follow(move || count.get() * 10);
    assert_eq!(reading.get(), 10);
    count.set(2);
    assert_eq!(reading.get(), 20);
}

/// A reading made to be kept reads nothing while it is made, and still follows its signal.
#[test]
fn a_reading_built_to_be_kept_arrives_as_it_was_made() {
    reset_runtime();
    let count = signal(1i32);
    let reading = Reactive::<i32>::follow(move || Reactive::of(move || count.get()));
    count.set(7);
    assert_eq!(reading.get(), 7);
    let handed = Reactive::<i32>::follow(move || count);
    count.set(8);
    assert_eq!(handed.get(), 8, "a signal handed over is followed");
}
