use super::*;

#[test]
fn a_letter_code_is_its_lowercase_letter() {
    assert_eq!(unmodified_key_of_code("KeyT"), Some(Key::Char('t')));
}

#[test]
fn a_digit_code_is_its_digit() {
    assert_eq!(unmodified_key_of_code("Digit1"), Some(Key::Char('1')));
}

#[test]
fn a_punctuation_code_is_its_character() {
    assert_eq!(unmodified_key_of_code("Equal"), Some(Key::Char('=')));
    assert_eq!(unmodified_key_of_code("Minus"), Some(Key::Char('-')));
}

#[test]
fn codes_that_type_nothing_name_no_key() {
    for code in ["", "ArrowUp", "Enter", "ShiftLeft", "KeyAB", "Key", "Digit"] {
        assert_eq!(unmodified_key_of_code(code), None, "{code}");
    }
}
