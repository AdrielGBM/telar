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
