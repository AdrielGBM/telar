use super::*;

#[test]
fn a_plain_name_is_only_quoted() {
    assert_eq!(css_string("Iosevka Term"), "\"Iosevka Term\"");
}

#[test]
fn a_quote_or_a_backslash_cannot_end_the_string() {
    assert_eq!(css_string(r#"Weird"Face\"#), r#""Weird\"Face\\""#);
}

#[test]
fn a_newline_cannot_end_the_string_and_start_a_declaration() {
    assert_eq!(css_string("Face\n;color:red"), "\"Face\\a ;color:red\"");
}

#[test]
fn control_characters_are_hex_escapes() {
    assert_eq!(
        css_string("a\r\t\u{0}\u{1b}\u{7f}\u{85}b"),
        "\"a\\d \\9 \\0 \\1b \\7f \\85 b\""
    );
}

#[test]
fn a_name_cannot_close_the_style_element_it_sits_in() {
    assert_eq!(css_string("a</style>"), "\"a\\3c /style>\"");
}

#[test]
fn text_beyond_ascii_is_written_as_it_is() {
    assert_eq!(css_string("Noto Sans 日本語 é"), "\"Noto Sans 日本語 é\"");
}
