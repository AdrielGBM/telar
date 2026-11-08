use super::*;

fn cfg(max_cols: u16) -> WrapConfig {
    WrapConfig {
        max_cols,
        wrap: true,
        max_lines: None,
        ellipsis: false,
    }
}

fn lines<'a>(text: &'a str, out: &[WrappedLine]) -> Vec<&'a str> {
    out.iter().map(|l| &text[l.range.clone()]).collect()
}

#[test]
fn breaks_at_spaces() {
    let text = "the quick brown fox";
    let mut out = Vec::new();
    wrap(text, &cfg(10), &mut out);
    assert_eq!(lines(text, &out), vec!["the quick", "brown fox"]);
}

#[test]
fn a_word_wider_than_the_column_is_cut() {
    let text = "abcdefghijkl";
    let mut out = Vec::new();
    wrap(text, &cfg(5), &mut out);
    assert_eq!(lines(text, &out), vec!["abcde", "fghij", "kl"]);
}

#[test]
fn hard_breaks_are_kept() {
    let text = "a\nb";
    let mut out = Vec::new();
    wrap(text, &cfg(80), &mut out);
    assert_eq!(lines(text, &out), vec!["a", "b"]);
}

#[test]
fn no_wrap_keeps_one_line_per_hard_break() {
    let text = "a very long line indeed";
    let mut out = Vec::new();
    wrap(
        text,
        &WrapConfig {
            wrap: false,
            ..cfg(5)
        },
        &mut out,
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].cols, 23);
}

#[test]
fn wide_glyphs_count_two_cells() {
    let text = "漢字漢字";
    let mut out = Vec::new();
    wrap(text, &cfg(4), &mut out);
    assert_eq!(lines(text, &out), vec!["漢字", "漢字"]);
}

#[test]
fn clamping_appends_an_ellipsis_within_the_column() {
    let text = "the quick brown fox";
    let mut out = Vec::new();
    wrap(
        text,
        &WrapConfig {
            max_lines: Some(1),
            ellipsis: true,
            ..cfg(10)
        },
        &mut out,
    );
    assert_eq!(out.len(), 1);
    assert!(
        out[0].ellipsized,
        "a clamped line is marked ellipsized: {:?}",
        out[0]
    );
    assert!(out[0].cols <= 10, "got {}", out[0].cols);
}

#[test]
fn clamping_without_an_ellipsis_just_drops_lines() {
    let text = "one two three four";
    let mut out = Vec::new();
    wrap(
        text,
        &WrapConfig {
            max_lines: Some(2),
            ..cfg(7)
        },
        &mut out,
    );
    assert_eq!(out.len(), 2);
    assert!(
        !out[1].ellipsized,
        "without an ellipsis the surviving line is untouched: {:?}",
        out[1]
    );
}

#[test]
fn empty_text_is_one_empty_line() {
    let mut out = Vec::new();
    wrap("", &cfg(10), &mut out);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].cols, 0);
}
