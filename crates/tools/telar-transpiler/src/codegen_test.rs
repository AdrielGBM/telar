use super::*;

/// A `//!` has to reach the generated file above its first item, or rustc answers `E0753` on code the author cannot edit. telar's own `#![allow]` sits beside it, since inner attributes are free to be ordered among themselves.
#[test]
fn a_module_root_keeps_the_authors_inner_attributes_on_top() {
    let out = transpile_module_root(
        "[logic]\n//! What is playing, on the bar.\n#![allow(dead_code)]\n\npub fn helper() -> u8 { 3 }\n",
        "__children.rs",
    )
    .expect("a module root with no markup");

    let first_item = out
        .rust_code
        .lines()
        .position(|line| line.starts_with("pub fn helper"))
        .expect("the body is emitted");
    assert!(
        !out.rust_code.contains("use crate::*"),
        "no prelude: a glob here makes the author's own imports ambiguous (E0659):\n{}",
        out.rust_code
    );
    let doc = out
        .rust_code
        .lines()
        .position(|line| line.starts_with("//!"))
        .expect("the author's doc survives");
    let attr = out
        .rust_code
        .lines()
        .position(|line| line.starts_with("#![allow(dead_code)]"))
        .expect("the author's inner attribute survives");
    assert!(doc < first_item && attr < first_item, "{}", out.rust_code);
    assert!(
        out.rust_code.contains("pub fn helper() -> u8 { 3 }"),
        "the body is at module level, not in a function: {}",
        out.rust_code
    );
    assert!(
        out.rust_code
            .trim_end()
            .ends_with("include!(\"__children.rs\");")
    );
}

/// Markup in a `mod.rsx` has no caller. Refusing beats ignoring: a dropped `[view]` is a component that renders nothing and says nothing.
#[test]
fn a_module_root_refuses_markup() {
    for (label, source) in [
        ("view", "[view]\ncol\n"),
        ("preview", "[preview \"One\"]\ncol\n"),
        ("style", "[style]\n@card\n  gap: 4\n"),
    ] {
        let error = transpile_module_root(source, "__children.rs")
            .expect_err(&format!("a `{label}` in a module root is an error"));
        assert!(
            error.to_string().contains("mod.rsx") || error.to_string().contains("[style]"),
            "{label}: {error}"
        );
    }
}

fn transpiled(source: &str) -> String {
    transpile_source(source, "demo", None, None)
        .expect("the component transpiles")
        .rust_code
}

#[test]
fn a_view_reading_scheme_binds_the_resolved_scheme_handle() {
    let code = transpiled(
        "[view]\nbox fill:(if $scheme == telar::ColorScheme::Dark { Color::BLACK } else { Color::WHITE })\n",
    );
    assert!(
        code.contains("let scheme = telar::ResolvedScheme;"),
        "{code}"
    );
    assert!(code.contains("scheme.get()"), "{code}");
}

#[test]
fn a_closure_reading_scheme_takes_the_handle() {
    let code = transpiled("[view]\nbox on_press:(|| log($scheme.get()))\n");
    assert!(
        code.contains("let scheme = telar::ResolvedScheme;"),
        "{code}"
    );
}

#[test]
fn a_view_that_never_reads_scheme_binds_nothing() {
    let code = transpiled("[view]\nbox width:10\n");
    assert!(!code.contains("ResolvedScheme"), "{code}");
}

#[test]
fn a_scheme_declared_in_logic_keeps_the_name() {
    let code = transpiled("[logic]\nlet scheme = memo(|| 3);\n\n[view]\ntext \"{$scheme}\"\n");
    assert!(!code.contains("ResolvedScheme"), "{code}");
    assert!(code.contains("let scheme = memo(|| 3);"), "{code}");
}

#[test]
fn a_preview_reading_scheme_binds_the_handle_in_its_own_fn() {
    let code = transpiled(
        "[view]\nbox width:10\n\n[preview \"Dark\"]\nbox fill:(if $scheme == telar::ColorScheme::Dark { Color::BLACK } else { Color::WHITE })\n",
    );
    let preview = code
        .split("fn demo_preview_0")
        .nth(1)
        .expect("the preview fn is emitted");
    assert!(
        preview.contains("let scheme = telar::ResolvedScheme;"),
        "{code}"
    );
}

#[test]
fn a_component_with_slots_reads_scheme_in_its_own_view_and_in_the_children_it_places() {
    let code = transpiled(
        "[logic]\nlet open = signal(true);\n\n[view]\ncol fill:(if $scheme == telar::ColorScheme::Dark { Color::BLACK } else { Color::WHITE })\n    if $open\n        children\n    panel\n        text \"{scheme_name($scheme)}\"\n",
    );
    syn::parse_file(&code).unwrap_or_else(|error| panic!("{error}:\n{code}"));
    let binding = "let scheme = telar::ResolvedScheme;";
    assert_eq!(code.matches(binding).count(), 1, "{code}");
    let bound = code.find(binding).expect("the handle is bound");
    let placed = code
        .find("children.build_slot(None)?")
        .unwrap_or_else(|| panic!("the placeholder builds its slot:\n{code}"));
    let nested = code
        .find("Children::per_slot(")
        .unwrap_or_else(|| panic!("the nested children are a per-slot recipe:\n{code}"));
    assert!(bound < placed && bound < nested, "{code}");
    let recipe = &code[nested..];
    assert!(recipe.contains("let scheme = scheme.clone();"), "{code}");
    assert!(recipe.contains("scheme_name(scheme.get())"), "{code}");
}

/// An item of `[logic]` lives at module level, where a `[style]` class, a test and another module can reach it; a statement stays in the component, where it can still read `props`.
#[test]
fn logic_items_are_hoisted_out_of_the_component() {
    let source = "[logic]\nconst GAP: f32 = 4.0;\n\n/// Twice the gap.\nfn double(x: f32) -> f32 {\n    x * 2.0\n}\n\n#[derive(Clone, Copy)]\nenum Side {\n    Start,\n}\n\nimpl Side {\n    fn flip(self) -> Self {\n        self\n    }\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn doubles() {\n        assert_eq!(super::double(GAP), 8.0);\n    }\n}\n\nlet width = double(GAP);\n[style]\n@card\n    gap: GAP * 2.0\n[view]\ncol @card width:width\n";
    let out = transpile_source(source, "demo", None, None).expect("transpiles");
    let code = &out.rust_code;
    let component = code.find("pub fn demo(").expect("the component");
    for item in [
        "const GAP: f32 = 4.0;",
        "fn double(x: f32) -> f32 {",
        "enum Side {",
        "impl Side {",
        "mod tests {",
        "fn style_card() -> LayoutStyle {",
    ] {
        let at = code
            .lines()
            .position(|line| line == item)
            .unwrap_or_else(|| panic!("`{item}` at column 0:\n{code}"));
        assert!(
            code.lines().take(at).map(|l| l.len() + 1).sum::<usize>() < component,
            "`{item}` above the component:\n{code}"
        );
    }
    assert!(code.contains("    let width = double(GAP);"), "{code}");
    assert!(code.contains("/// Twice the gap.\nfn double"), "{code}");
    assert!(code.contains("#[cfg(test)]\nmod tests {"), "{code}");
    let line_of = |needle: &str| {
        code.lines()
            .position(|line| line.contains(needle))
            .and_then(|at| out.source_map[at])
    };
    assert_eq!(
        line_of("const GAP"),
        Some(1),
        "a hoisted line keeps its origin"
    );
    assert_eq!(line_of("fn double"), Some(4));
    assert_eq!(line_of("let width"), Some(27));
}

/// A statement that only looks like an item, and an item nested in a block, stay where they were written.
#[test]
fn only_column_zero_items_are_hoisted() {
    let code = transpiled(
        "[logic]\nlet unsafe_count = 3;\nlet total = {\n    const STEP: u32 = 2;\n    STEP * unsafe_count\n};\n[view]\ntext \"{total}\"\n",
    );
    assert!(code.contains("    let unsafe_count = 3;"), "{code}");
    assert!(code.contains("        const STEP: u32 = 2;"), "{code}");
}

/// A `.rsx` that declares classes and no view is a module of them: its items at module level, no component, and no function for a statement to run in.
#[test]
fn a_viewless_rsx_is_a_module_of_classes() {
    let code = transpiled(
        "[logic]\nconst RULE: f32 = 1.0;\n[style]\npub @line\n    stroke_bottom: RULE\n    gap: 8\n",
    );
    assert!(!code.contains("pub fn demo("), "no component:\n{code}");
    assert!(
        !code.contains("fn style_line"),
        "the importers build it:\n{code}"
    );
    assert!(code.contains("const RULE: f32 = 1.0;"), "{code}");
    assert!(!code.contains("compile_error!"), "{code}");

    let code = transpiled("[logic]\nlet x = 1;\n[style]\npub @line\n    gap: 8\n");
    assert!(
        code.contains("compile_error!"),
        "a statement is refused:\n{code}"
    );
    let code = transpiled("[style]\npub @line\n    align: centre\n");
    assert!(
        code.contains("compile_error!") && code.contains("in `@line`"),
        "its classes are still checked where they are written:\n{code}"
    );
}

/// A directory's `mod.rsx` may hold the classes its modules share, and only those.
#[test]
fn a_module_root_shares_pub_classes() {
    let out = transpile_module_root("[style]\npub @card\n    gap: 4\n", "__children.rs")
        .expect("a pub class has importers");
    assert!(
        !out.rust_code.contains("compile_error!"),
        "{}",
        out.rust_code
    );
    let error = transpile_module_root("[style]\n@card\n    gap: 4\n", "__children.rs")
        .expect_err("a private class has no reader");
    assert!(error.to_string().contains("pub @card"), "{error}");
}

/// A class no `[style]` declares or imports is an error on the element naming it, not a box styled by nothing.
#[test]
fn an_unknown_class_is_refused_where_it_is_named() {
    let out = transpile_source("[view]\ncol @missing gap:4\n", "demo", None, None).unwrap();
    let at = out
        .rust_code
        .lines()
        .position(|line| line.contains("no class `@missing` here"))
        .expect("the class is named");
    assert_eq!(out.source_map[at], Some(1), "{}", out.rust_code);
    assert!(
        !out.rust_code.contains("style_missing()"),
        "{}",
        out.rust_code
    );
}

/// Outside a package there is no module to read an import from, and the `use` says so on its line.
#[test]
fn an_import_outside_a_package_is_refused_on_its_line() {
    let out = transpile_source(
        "[style]\nuse crate::styles::mono;\n[view]\ntext @mono \"x\"\n",
        "demo",
        None,
        None,
    )
    .unwrap();
    let at = out
        .rust_code
        .lines()
        .position(|line| line.contains("names a module of the package"))
        .expect("the import is refused");
    assert_eq!(out.source_map[at], Some(1));
}
