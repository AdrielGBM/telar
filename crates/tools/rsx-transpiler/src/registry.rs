//! Authoritative registry of built-in RSX tags and layout attribute keys.
//!
//! These tables are the single source of truth shared between the transpiler's
//! codegen (`view.rs`, `style.rs`) and downstream tooling such as `rsx-analyzer`
//! (completions, hover, go-to-definition). Keep them in sync with the `match`
//! arms in [`crate::view`] (`emit_element`) and [`crate::style`] (`layout_prop_call`).

/// Built-in RSX tags paired with the Rust constructor path they transpile to.
///
/// Mirrors the tag dispatch in `ViewGen::emit_element`. Tags that share a
/// constructor (e.g. `col`/`row`/`grid` -> `Container::new`) are listed once per
/// spelling so lookups by tag name resolve every alias.
pub fn builtin_tags() -> &'static [(&'static str, &'static str)] {
    &[
        ("text", "Text::new"),
        ("btn", "Button::new"),
        ("button", "Button::new"),
        ("col", "Container::new"),
        ("column", "Container::new"),
        ("row", "Container::new"),
        ("grid", "Container::new"),
        ("box", "StyledContainer::new"),
        ("img", "Image::new"),
        ("image", "Image::new"),
        ("scroll", "LayoutScrollArea::new"),
        ("canvas", "Canvas::new"),
        ("widget", "WidgetRef"),
    ]
}

/// Layout attribute keys common to every container-like tag.
///
/// Mirrors the recognized `match` arms in `style::layout_prop_call` that map to
/// `LayoutStyle` builder calls, excluding the grid-only keys (`cols`, `span`,
/// `row-span`) which downstream tooling offers solely on the `grid` tag.
/// Aliases (`pad`/`padding`) are listed individually so completion offers both.
pub fn layout_attr_keys() -> &'static [&'static str] {
    &[
        "width",
        "height",
        "min-width",
        "min-height",
        "padding",
        "pad",
        "padding-x",
        "pad-x",
        "padding-y",
        "pad-y",
        "gap",
        "gap-x",
        "gap-y",
        "grow",
        "shrink",
        "direction",
        "align",
        "justify",
    ]
}
