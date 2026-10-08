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

#[test]
fn preview_slug_lowercases_and_joins_words_with_one_hyphen() {
    assert_eq!(preview_slug("Landing — full page"), "landing-full-page");
    assert_eq!(preview_slug("Counting presses"), "counting-presses");
    assert_eq!(preview_slug("snake_case/and.dots"), "snake-case-and-dots");
}

#[test]
fn preview_slug_trims_separators_at_either_end() {
    assert_eq!(preview_slug("  (Default)  "), "default");
    assert_eq!(preview_slug("--x--"), "x");
}

#[test]
fn preview_slug_keeps_letters_beyond_ascii() {
    assert_eq!(preview_slug("Größe 2"), "größe-2");
}

#[test]
fn preview_slug_of_a_name_with_no_letters_is_empty() {
    assert_eq!(preview_slug("— · —"), "");
}

#[test]
fn preview_id_suffix_names_the_component_and_the_slug() {
    assert_eq!(
        preview_id_suffix("button", "Primary — large"),
        Ok("--button--primary-large".to_string())
    );
}

#[test]
fn preview_id_suffix_refuses_a_name_with_nothing_to_slug() {
    let err = preview_id_suffix("button", "—").unwrap_err();
    assert_eq!(
        err,
        PreviewIdError::NoLetterOrDigit {
            name: "—".to_string()
        }
    );
    assert_eq!(
        err.to_string(),
        "preview \"—\" needs a letter or digit in its name to form its id"
    );
}

#[test]
fn preview_file_expr_joins_the_manifest_dir_and_the_package_path() {
    assert_eq!(
        preview_file_expr("src/card.rsx"),
        r#"concat!(env!("CARGO_MANIFEST_DIR"), "/src/card.rsx")"#
    );
    assert_eq!(
        preview_file_expr("/src/card.rsx"),
        preview_file_expr("src/card.rsx")
    );
}
