use super::*;

#[test]
fn separators_inside_parens_and_brackets_stay() {
    assert_eq!(
        split_top_level("a(1, 2), [x, y], b", |c| c == ',', true),
        ["a(1, 2)", " [x, y]", " b"]
    );
}

#[test]
fn separators_inside_quotes_stay() {
    assert_eq!(
        split_top_level(r#"label:"a b" "c \" d""#, char::is_whitespace, false),
        [r#"label:"a b""#, r#""c \" d""#]
    );
}

#[test]
fn empty_segments_are_kept_or_dropped() {
    assert_eq!(
        split_top_level("a,,b,", |c| c == ',', true),
        ["a", "", "b", ""]
    );
    assert_eq!(split_top_level("a,,b,", |c| c == ',', false), ["a", "b"]);
}

#[test]
fn whitespace_runs_make_one_boundary() {
    assert_eq!(
        split_top_level("  a   b(1 2) ", char::is_whitespace, false),
        ["a", "b(1 2)"]
    );
}

#[test]
fn multibyte_text_splits_on_char_boundaries() {
    assert_eq!(split_top_level("é,ü", |c| c == ',', true), ["é", "ü"]);
}
