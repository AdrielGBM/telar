use super::*;

#[test]
fn every_key_name_is_one_the_runtime_reads() {
    for (name, _) in KEY_VALUES {
        assert!(
            semantics_core::ConsumedKeys::named(name).is_some(),
            "`{name}` is offered but `ConsumedKeys` does not read it"
        );
    }
}

#[test]
fn every_single_key_the_runtime_writes_can_be_named() {
    let all = semantics_core::ConsumedKeys::SCROLLING
        | semantics_core::ConsumedKeys::TAB
        | semantics_core::ConsumedKeys::ENTER
        | semantics_core::ConsumedKeys::BACKSPACE;
    for name in all.to_names().split(' ') {
        assert!(
            key_constant(name).is_some(),
            "`{name}` has no spelling here"
        );
    }
}

#[test]
fn the_attribute_is_offered_on_a_box() {
    assert!(attr_spec("box", "consumes_keys").is_some_and(|spec| spec.doc.is_some()));
}
