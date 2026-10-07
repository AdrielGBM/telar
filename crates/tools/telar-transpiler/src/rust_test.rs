use super::*;

fn idents(expr: &str) -> Vec<String> {
    free_idents(expr).unwrap_or_else(|| panic!("`{expr}` did not parse"))
}

#[test]
fn a_field_a_method_and_a_path_segment_are_not_bindings() {
    assert_eq!(idents("seat(&desk, id).x"), ["seat", "desk", "id"]);
    assert_eq!(idents("crate::scale::md()"), Vec::<String>::new());
    assert_eq!(idents("value.clamp(lo, hi)"), ["value", "lo", "hi"]);
    assert_eq!(idents("Style { pad, ..base }"), ["pad", "base"]);
}

#[test]
fn a_closure_parameter_shadows_the_scope_it_sits_in() {
    assert_eq!(
        idents("items.iter().map(|x| x + offset)"),
        ["items", "offset"]
    );
    assert_eq!(idents("|n| { let m = n * 2; m + k }"), ["k"]);
}

#[test]
fn a_pattern_binds_every_name_it_introduces() {
    assert_eq!(
        idents("match slot { Some((a, b)) => a + b, None => fallback }"),
        ["slot", "fallback"]
    );
}

/// `$` is the markup's sugar and not Rust, so a value still carrying one has to be substituted before it can be read — saying so is what keeps the caller's fallback scan reachable.
#[test]
fn text_that_is_not_an_expression_says_so_instead_of_guessing() {
    assert_eq!(free_idents("$count.get()"), None);
    assert_eq!(free_idents("col gap:8"), None);
    assert_eq!(idents("12px"), Vec::<String>::new());
}

/// What the clone prelude has to leave alone: a view `let` declares inside the closure being wrapped, so its names are the closure's own rather than captures from around it.
#[test]
fn a_let_reports_the_names_it_binds() {
    assert_eq!(
        let_bindings("let rect = chip_rect(&rects, m)").unwrap(),
        ["rect"]
    );
    assert_eq!(
        let_bindings("let (a, mut b): (u8, u8) = pair;").unwrap(),
        ["a", "b"]
    );
    assert_eq!(let_bindings("chip_rect(&rects, m)"), None);
}

fn moved(code: &str, names: &[&str]) -> Vec<(String, bool)> {
    moved_reads(code, names)
        .unwrap_or_else(|| panic!("`{code}` did not parse"))
        .into_iter()
        .map(|read| {
            assert_eq!(&code[read.offset..read.offset + read.name.len()], read.name);
            (code[..read.offset].to_string(), read.shorthand)
        })
        .collect()
}

fn moved_at(code: &str, name: &str) -> Vec<usize> {
    moved_reads(code, &[name])
        .unwrap_or_else(|| panic!("`{code}` did not parse"))
        .into_iter()
        .map(|read| read.offset)
        .collect()
}

#[test]
fn only_a_read_inside_a_move_is_a_capture() {
    let code = "let a = (x.get(), memo(move || x.get()), x.get());";
    assert_eq!(moved_at(code, "x"), [code.find("x.get())").unwrap()]);
    assert_eq!(
        moved_at("let a = memo(|| x.get());", "x"),
        Vec::<usize>::new()
    );
    assert_eq!(moved_at("spawn(async move { x.get() });", "x"), [19]);
}

#[test]
fn a_field_name_in_a_struct_literal_is_not_a_read() {
    let code = "let f = move || Config {\n    x: x.peek(),\n    y,\n};";
    let reads = moved(code, &["x", "y"]);
    assert_eq!(reads.len(), 2, "{reads:?}");
    assert!(reads[0].0.ends_with("x: ") && !reads[0].1, "{reads:?}");
    assert!(
        reads[1].0.ends_with("    ") && reads[1].1,
        "the shorthand: {reads:?}"
    );
}

#[test]
fn a_name_that_is_not_the_binding_is_not_a_read() {
    for code in [
        "let f = move || s.x.get();",
        "let f = move || s.x();",
        "let f = move || x::new();",
        "let f = move || m::x::new();",
        "let f = move || x!();",
        "let f = move || 'x: loop { break 'x; };",
        "let f = move || format!(\"{x}\", x = 1);",
        "let f = move || { fn g(x: u8) -> u8 { x } g(1) };",
    ] {
        assert_eq!(moved_at(code, "x"), Vec::<usize>::new(), "{code}");
    }
}

#[test]
fn a_binding_made_inside_the_closure_shadows_the_capture() {
    for code in [
        "let f = move |x| x + 1;",
        "let f = move || { let x = 1; x };",
        "let f = move |p: P| { let P { x, .. } = p; x };",
        "let f = move |o: Option<u8>| if let Some(x) = o { x } else { 0 };",
        "let f = move |o: Option<u8>| match o { Some(x) if x > 1 => x, _ => 0 };",
        "let f = move |v: Vec<u8>| { for x in v { drop(x); } };",
    ] {
        assert_eq!(moved_at(code, "x"), Vec::<usize>::new(), "{code}");
    }
    let code = "let f = move |o: Option<u8>| { if let Some(x) = o { x } else { x.get() } };";
    assert_eq!(moved_at(code, "x"), [code.rfind("x.get()").unwrap()]);
}

#[test]
fn a_macro_argument_is_a_read() {
    let code = "let f = move || println!(\"{} {y}\", x.get(), y = x.peek());";
    assert_eq!(moved_at(code, "x").len(), 2, "{code}");
    let code = "let f = move || custom!(x => x.get());";
    assert_eq!(moved_at(code, "x").len(), 2, "{code}");
}

#[test]
fn a_name_inside_a_literal_or_comment_is_not_a_read() {
    let code = "let f = move || { \"x\"; 'x'; // x\n x.get() };";
    assert_eq!(moved_at(code, "x"), [code.rfind("x.get()").unwrap()]);
}

#[test]
fn code_that_does_not_parse_says_so() {
    assert_eq!(moved_reads("let f = move || x.get(", &["x"]), None);
    assert_eq!(moved_reads("let f = 1;", &["x"]), Some(Vec::new()));
}

fn renamed_in_format(code: &str) -> String {
    let mut renamed = code.to_string();
    for read in moved_reads(code, &["x"]).unwrap().into_iter().rev() {
        renamed.replace_range(read.offset..read.offset + 1, "x2");
    }
    renamed
}

#[test]
fn a_name_a_format_string_captures_is_a_read_in_every_format_macro() {
    for (code, expected) in [
        ("move || format!(\"{x}\")", "move || format!(\"{x2}\")"),
        (
            "move || format_args!(\"{x:?}\")",
            "move || format_args!(\"{x2:?}\")",
        ),
        (
            "move || println!(\"{x:>8}\")",
            "move || println!(\"{x2:>8}\")",
        ),
        ("move || eprintln!(\"{x}\")", "move || eprintln!(\"{x2}\")"),
        ("move || print!(\"{x}\")", "move || print!(\"{x2}\")"),
        ("move || panic!(\"{x}\")", "move || panic!(\"{x2}\")"),
        (
            "move || unreachable!(\"{x}\")",
            "move || unreachable!(\"{x2}\")",
        ),
        (
            "move || write!(out, \"{x}\")",
            "move || write!(out, \"{x2}\")",
        ),
        (
            "move || writeln!(out, \"{x}\")",
            "move || writeln!(out, \"{x2}\")",
        ),
        (
            "move || assert!(ok, \"{x}\")",
            "move || assert!(ok, \"{x2}\")",
        ),
        (
            "move || assert_eq!(a, b, \"{x}\")",
            "move || assert_eq!(a, b, \"{x2}\")",
        ),
        (
            "move || debug_assert_ne!(a, b, \"{x}\")",
            "move || debug_assert_ne!(a, b, \"{x2}\")",
        ),
        (
            "move || tracing::info!(\"{x}\")",
            "move || tracing::info!(\"{x2}\")",
        ),
        (
            "move || warn!(target: \"t\", n = 1, \"{x}\")",
            "move || warn!(target: \"t\", n = 1, \"{x2}\")",
        ),
        (
            "move || anyhow::bail!(\"{x}\")",
            "move || anyhow::bail!(\"{x2}\")",
        ),
        (
            "move || format!(\"{:w$.p$}\", 1.0, w = 4, p = x)",
            "move || format!(\"{:w$.p$}\", 1.0, w = 4, p = x2)",
        ),
        (
            "move || format!(\"{:x$}\", 1)",
            "move || format!(\"{:x2$}\", 1)",
        ),
    ] {
        assert_eq!(renamed_in_format(code), expected, "{code}");
    }
}

#[test]
fn a_format_string_names_nothing_it_does_not_capture() {
    for code in [
        "move || format!(\"{{x}}\")",
        "move || format!(\"{{x}} {}\", 1)",
        "move || format!(\"{0}\", 1)",
        "move || format!(\"{x}\", x = 1)",
        "move || assert_eq!(\"{x}\", s)",
        "move || custom!(\"{x}\")",
        "move || t!(\"key\", x = 1)",
        "|| format!(\"{x}\")",
        "move |x: u8| format!(\"{x}\")",
        "move || { let s = \"{x}\"; s }",
    ] {
        assert_eq!(renamed_in_format(code), code, "{code}");
    }
    assert_eq!(
        renamed_in_format("move || format!(\"{{{x}}}\")"),
        "move || format!(\"{{{x2}}}\")"
    );
}

#[test]
fn free_idents_sees_a_name_a_format_string_captures() {
    assert_eq!(
        idents("format!(\"{label}: {} {{n}}\", value)"),
        ["value", "label"]
    );
}
