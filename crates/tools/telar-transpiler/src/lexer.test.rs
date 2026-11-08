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

fn callees(expr: &str) -> Vec<(Callee, &str)> {
    handed_closures(expr)
        .into_iter()
        .map(|closure| (closure.callee, &expr[closure.span]))
        .collect()
}

fn path(name: &str) -> Callee {
    Callee::Path(name.to_string())
}

fn method(name: &str) -> Callee {
    Callee::Method(name.to_string())
}

#[test]
fn a_closure_handed_to_a_call_is_listed_with_its_callee() {
    assert_eq!(
        callees("Reactive::of(move || sig.get()) + other.get()"),
        [(path("Reactive::of"), "move || sig.get()")]
    );
    assert_eq!(
        callees("items.iter().map(|i| i + sig.get())"),
        [(method("map"), "|i| i + sig.get()")]
    );
    assert_eq!(
        callees("x.map_or(0.0, |v| v * 2.0)"),
        [(method("map_or"), "|v| v * 2.0")]
    );
    assert!(callees("a.get() | b.get()").is_empty());
}

#[test]
fn a_closure_inside_another_is_listed_after_it() {
    assert_eq!(
        callees("helper(|| Rc::new(move || x.get()))"),
        [
            (path("helper"), "|| Rc::new(move || x.get())"),
            (path("Rc::new"), "move || x.get()"),
        ]
    );
}

#[test]
fn generic_arguments_do_not_hide_the_callee() {
    assert_eq!(
        callees("Reactive::<f32>::of(move || sig.get())")[0].0,
        path("Reactive::of")
    );
    assert_eq!(
        callees("Rc::<dyn Fn() -> f32>::new(move || sig.get())")[0].0,
        path("Rc::new")
    );
    assert_eq!(callees("v.iter().map::<f32, _>(|x| x)")[0].0, method("map"));
}

#[test]
fn a_closure_handed_to_no_callee_is_not_listed() {
    for expr in [
        "a > (|| sig.get())",
        "if (|| x)() { 1 } else { 2 }",
        "vec![|| x.get()]",
        "format!(\"{}\", (|| x.get())())",
    ] {
        assert!(callees(expr).is_empty(), "{expr}");
    }
}
