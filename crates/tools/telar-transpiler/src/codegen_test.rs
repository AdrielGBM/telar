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
