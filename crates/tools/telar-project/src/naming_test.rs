use super::*;

#[test]
fn snake_basic_already_snake() {
    assert_eq!(to_snake_case("btn_primary"), "btn_primary");
    assert_eq!(to_snake_case("hello_world"), "hello_world");
}

#[test]
fn snake_hyphen_separator() {
    assert_eq!(to_snake_case("my-component"), "my_component");
    assert_eq!(to_snake_case("card-title"), "card_title");
}

#[test]
fn snake_dot_separator() {
    assert_eq!(to_snake_case("btn.primary"), "btn_primary");
    assert_eq!(to_snake_case("info.card"), "info_card");
}

#[test]
fn snake_space_separator() {
    assert_eq!(to_snake_case("my component"), "my_component");
}

#[test]
fn snake_consecutive_separators_collapsed() {
    assert_eq!(to_snake_case("a--b"), "a_b");
    assert_eq!(to_snake_case("a._b"), "a_b");
}

#[test]
fn snake_leading_digit_prefixed() {
    assert_eq!(to_snake_case("3d"), "_3d");
    assert_eq!(to_snake_case("2fast"), "_2fast");
}

#[test]
fn snake_strips_unknown_chars() {
    assert_eq!(to_snake_case("btn@primary"), "btnprimary");
}

#[test]
fn pascal_basic_already_pascal() {
    assert_eq!(to_pascal_case("BtnPrimary"), "BtnPrimary");
    assert_eq!(to_pascal_case("HelloWorld"), "HelloWorld");
}

#[test]
fn pascal_snake_input() {
    assert_eq!(to_pascal_case("btn_primary"), "BtnPrimary");
    assert_eq!(to_pascal_case("hello_world"), "HelloWorld");
}

#[test]
fn pascal_hyphen_separator() {
    assert_eq!(to_pascal_case("my-component"), "MyComponent");
}

#[test]
fn pascal_dot_separator() {
    assert_eq!(to_pascal_case("info.card"), "InfoCard");
    assert_eq!(to_pascal_case("btn.primary"), "BtnPrimary");
}

#[test]
fn pascal_leading_digit_prefixed() {
    assert_eq!(to_pascal_case("3d"), "_3d");
}

#[test]
fn pascal_strips_non_alphanumeric_non_sep() {
    assert_eq!(to_pascal_case("info@card"), "Infocard");
}

#[test]
fn pascal_single_word() {
    assert_eq!(to_pascal_case("primary"), "Primary");
    assert_eq!(to_pascal_case("card"), "Card");
}
