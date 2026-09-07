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
fn replace_whole_word_leaves_literals_and_comments_intact() {
    assert_eq!(
        replace_whole_word("charging.get()", "charging", "c2"),
        "c2.get()"
    );
    assert_eq!(
        replace_whole_word("charging = \"battery-charging\"", "charging", "c2"),
        "c2 = \"battery-charging\""
    );
    assert_eq!(
        replace_whole_word("charging.set(0) // charging", "charging", "c2"),
        "c2.set(0) // charging"
    );
    assert_eq!(
        replace_whole_word("charging_glyph", "charging", "c2"),
        "charging_glyph"
    );
}

#[test]
fn replace_whole_word_leaves_a_struct_literal_field_alone() {
    // Regression: a field and the signal holding it share a name, and the clone rewrite renamed both, leaving a struct literal naming a field that does not exist.
    assert_eq!(
        replace_whole_word("Config { vim: vim.peek() }", "vim", "vim_rsx_mv"),
        "Config { vim: vim_rsx_mv.peek() }"
    );
    assert_eq!(
        replace_whole_word("C { a: 1, vim: vim.peek() }", "vim", "v2"),
        "C { a: 1, vim: v2.peek() }"
    );
    assert_eq!(
        replace_whole_word("let vim: bool = vim.peek();", "vim", "v2"),
        "let v2: bool = v2.peek();"
    );
    assert_eq!(replace_whole_word("vim::set()", "vim", "v2"), "v2::set()");
}

#[test]
fn replace_whole_word_leaves_a_field_of_the_same_name_alone() {
    // Regression: `let tool = memo(move || store().tool.get())` renamed the field and produced `no field tool_rsx_mv` against generated code.
    assert_eq!(
        replace_whole_word("store().tool.get()", "tool", "tool_rsx_mv"),
        "store().tool.get()"
    );
    assert_eq!(
        replace_whole_word("tool.set(s.tool.get())", "tool", "t2"),
        "t2.set(s.tool.get())"
    );
    assert_eq!(replace_whole_word("x.count()", "count", "c2"), "x.count()");
    assert_eq!(
        replace_whole_word("Config { ..base }", "base", "b2"),
        "Config { ..b2 }"
    );
    assert_eq!(replace_whole_word("0..count", "count", "c2"), "0..c2");
}
