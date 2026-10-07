use super::*;

// Regression: `//` skipped to the end of the whole snippet, so on a multi-line `[logic]` block every signal declared after the first comment read as absent.
#[test]
fn a_line_comment_hides_only_its_own_line() {
    assert!(
        contains_ident("// count\nlet x = count;", "count"),
        "a line comment ends at its newline, not at the end of the snippet"
    );
    assert!(
        !contains_ident("// count is gone", "count"),
        "an identifier inside a comment is not a use"
    );
    assert!(
        contains_ident("let a = 1; /* count */ let b = count;", "count"),
        "a block comment ends at its `*/`"
    );
    assert!(
        !contains_ident("/* count */", "count"),
        "a lone block comment holds no code"
    );
    assert!(
        !contains_ident(r#"let s = r"count";"#, "count"),
        "a raw string is text, not code"
    );
    assert!(
        contains_ident(r##"let s = r#"x"#; let y = count;"##, "count"),
        "the raw string ends at its closing hash, so what follows is code"
    );
}

#[test]
fn contains_ident_skips_literals_and_comments() {
    assert!(
        contains_ident("charging.get()", "charging"),
        "a plain method call is a use"
    );
    assert!(
        !contains_ident("charging_glyph.get()", "charging"),
        "prefix is not a whole word"
    );
    assert!(
        !contains_ident("if c { \"battery-charging\" } else { \"x\" }", "charging"),
        "the word inside a string literal is not a use"
    );
    assert!(
        !contains_ident("x + 1 // reset charging", "charging"),
        "comment is not code"
    );
    assert!(
        contains_ident("if c == 'x' { charging.set(true) }", "charging"),
        "a char literal does not swallow the rest of the line"
    );
}

#[test]
fn ident_positions_finds_every_whole_word_outside_literals() {
    let code = "count + \"count\" + counter + r#count // count\ncount";
    assert_eq!(
        ident_positions(code, "count").collect::<Vec<_>>(),
        [0, 30, 45]
    );
}

#[test]
fn format_arg_positions_finds_only_names_in_argument_position_inside_strings() {
    let code = "x(\"{x} {x:?} {:x$} {xy} x\", r\"{x}\")";
    assert_eq!(format_arg_positions(code, "x"), [4, 8, 15, 31]);
}
