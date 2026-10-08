use super::*;

fn round_trip<T: PreviewArg + PartialEq + std::fmt::Debug>(value: T) {
    let erased = value.to_arg_value();
    let text = erased.to_string();
    let parsed: ArgValue = text.parse().expect("the text form reads back");
    assert_eq!(
        T::from_arg_value(&parsed).as_ref(),
        Some(&value),
        "through `{text}`"
    );
}

#[test]
fn every_primitive_round_trips_through_text() {
    round_trip(true);
    round_trip(-7i8);
    round_trip(i16::MIN);
    round_trip(i32::MAX);
    round_trip(i64::MIN);
    round_trip(i128::MAX);
    round_trip(isize::MIN);
    round_trip(u8::MAX);
    round_trip(u16::MAX);
    round_trip(u32::MAX);
    round_trip(u64::MAX);
    round_trip(usize::MAX);
    round_trip(0.1f32);
    round_trip(f32::MAX);
    round_trip(-2.5f64);
    round_trip(String::from("Save \"all\""));
    round_trip("Save");
    round_trip(Color::rgba(0.1, 0.2, 0.3, 0.4));
    round_trip(Some(3u8));
    round_trip(None::<u8>);
    round_trip(Some(Some(true)));
}

#[test]
fn the_controls_follow_the_type() {
    assert_eq!(bool::CONTROL, ControlKind::Toggle);
    assert_eq!(u8::CONTROL, ControlKind::INTEGER);
    assert_eq!(f32::CONTROL, ControlKind::FLOAT);
    assert_eq!(String::CONTROL, ControlKind::TEXT);
    assert_eq!(<&'static str>::CONTROL, ControlKind::TEXT);
    assert_eq!(Color::CONTROL, ControlKind::Color);
    assert_eq!(
        Option::<bool>::CONTROL,
        ControlKind::Optional(&ControlKind::Toggle)
    );
    assert_eq!(Reactive::<String>::CONTROL, ControlKind::TEXT);
    assert_eq!(RwSignal::<f32>::CONTROL, ControlKind::FLOAT);
}

#[test]
fn a_number_crosses_between_integer_and_float_when_it_fits() {
    assert_eq!(f32::from_arg_value(&ArgValue::Int(3)), Some(3.0));
    assert_eq!(u8::from_arg_value(&ArgValue::Float(3.0)), Some(3));
    assert_eq!(u8::from_arg_value(&ArgValue::Float(3.5)), None);
    assert_eq!(u8::from_arg_value(&ArgValue::Int(256)), None);
    assert_eq!(u8::from_arg_value(&ArgValue::Int(-1)), None);
    assert_eq!(i32::from_arg_value(&ArgValue::Float(f64::NAN)), None);
}

#[test]
fn a_value_of_another_kind_is_refused() {
    assert_eq!(bool::from_arg_value(&ArgValue::Int(1)), None);
    assert_eq!(String::from_arg_value(&ArgValue::Choice("A".into())), None);
    assert!(!<&'static str>::accepts(&ArgValue::Bool(true)));
    assert_eq!(Option::<bool>::from_arg_value(&ArgValue::Unset), Some(None));
    assert!(Option::<bool>::accepts(&ArgValue::Unset));
    assert!(!bool::accepts(&ArgValue::Unset));
}

/// A control sending the same text back on every keystroke must not leak a fresh copy each time.
#[test]
fn a_static_str_override_leaks_each_text_once() {
    let text = ArgValue::Text("leaked once".into());
    let first = <&'static str>::from_arg_value(&text).unwrap();
    let second = <&'static str>::from_arg_value(&text).unwrap();
    assert_eq!(first, "leaked once");
    assert!(std::ptr::eq(first, second));
}

#[test]
fn a_reactive_override_is_held_as_a_constant() {
    let value = Reactive::<String>::from_arg_value(&ArgValue::Text("Hi".into())).unwrap();
    assert!(matches!(value, Reactive::Const(ref text) if text == "Hi"));
    assert_eq!(
        Reactive::of(|| String::from("Read")).to_arg_value(),
        ArgValue::Text("Read".into())
    );
}

#[test]
fn a_signal_override_is_a_signal_holding_it() {
    let held = RwSignal::<u32>::from_arg_value(&ArgValue::Int(9)).unwrap();
    assert_eq!(held.peek(), 9);
    assert_eq!(held.to_arg_value(), ArgValue::Int(9));
}
