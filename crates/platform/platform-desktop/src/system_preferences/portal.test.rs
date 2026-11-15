use super::*;

#[test]
fn an_unsigned_integer_is_read_as_it_is() {
    assert_eq!(as_u32(&Value::U32(2)), Some(2));
    assert_eq!(as_u32(&Value::U32(u32::MAX)), Some(u32::MAX));
    assert_eq!(as_u32(&Value::U8(1)), Some(1));
}

#[test]
fn a_signed_integer_is_read_when_it_is_not_negative() {
    assert_eq!(as_u32(&Value::I32(1)), Some(1));
    assert_eq!(as_u32(&Value::I32(0)), Some(0));
    assert_eq!(as_u32(&Value::I32(-1)), None);
}

#[test]
fn a_value_wrapped_in_variants_is_read_through_every_wrapper() {
    let once = Value::Value(Box::new(Value::U32(1)));
    let twice = Value::Value(Box::new(Value::Value(Box::new(Value::I32(2)))));

    assert_eq!(as_u32(&once), Some(1));
    assert_eq!(as_u32(&twice), Some(2));
    assert_eq!(
        as_u32(&Value::Value(Box::new(Value::I32(-3)))),
        None,
        "a wrapper must not make a negative value readable"
    );
}

#[test]
fn a_value_of_another_type_is_not_a_number() {
    assert_eq!(as_u32(&Value::Bool(true)), None);
    assert_eq!(as_u32(&Value::Str("1".into())), None);
    assert_eq!(as_u32(&Value::I64(1)), None);
    assert_eq!(as_u32(&Value::Value(Box::new(Value::Bool(false)))), None);
}

#[test]
fn the_text_scale_is_read_from_the_interface_namespace_alone() {
    let scaled = changed_setting(INTERFACE_NAMESPACE, TEXT_SCALING_FACTOR, &Value::F64(1.25));
    let mut preferences = SystemPreferences::default();
    scaled.expect("a text scale").apply(&mut preferences);
    assert_eq!(preferences.text_scale.get(), 1.25);

    assert_eq!(
        changed_setting(APPEARANCE_NAMESPACE, TEXT_SCALING_FACTOR, &Value::F64(1.25)),
        None
    );
    assert_eq!(
        changed_setting(INTERFACE_NAMESPACE, "cursor-size", &Value::F64(1.25)),
        None
    );
}
