use super::*;

fn prop<T>(value: impl PropNumber<T>) -> T {
    value.into_prop()
}

#[test]
fn a_whole_number_is_whatever_numeric_prop_takes_it() {
    assert_eq!(prop::<u8>(Int(3)), 3);
    assert_eq!(prop::<i32>(Int(-2)), -2);
    assert_eq!(prop::<f32>(Int(300)), 300.0);
}

#[test]
fn a_number_with_a_fraction_is_a_float_prop() {
    assert_eq!(prop::<f32>(Float(1.5)), 1.5);
    assert_eq!(prop::<f64>(Float(0.25)), 0.25);
}

/// A Rust literal still types to the prop exactly, since the prop's own type is the only integer or float among what it takes.
#[test]
fn a_rust_literal_is_the_prop_s_own_type() {
    assert_eq!(prop::<u8>(7), 7u8);
    assert_eq!(prop::<f32>(2.5), 2.5f32);
}

#[test]
#[should_panic(expected = "`300` is out of range for a `u8` prop")]
fn a_whole_number_out_of_the_prop_s_range_says_so() {
    prop::<u8>(Int(300));
}

#[test]
fn a_reactive_prop_takes_a_markup_number_too() {
    assert_eq!(Reactive::<usize>::from(Int(4)).get(), 4);
    assert_eq!(Reactive::<f32>::from(Float(0.5)).get(), 0.5);
    assert_eq!(Reactive::<f32>::from(Int(2)).get(), 2.0);
}
