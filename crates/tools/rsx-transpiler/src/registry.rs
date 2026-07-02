//! Authoritative registry of built-in RSX tags and layout attribute keys.
//!
//! These tables are the single source of truth shared between the transpiler's codegen (`view.rs`, `style.rs`) and downstream tooling such as `rsx-analyzer` (completions, hover, go-to-definition). Keep them in sync with the `match` arms in [`crate::view`] (`emit_element`) and [`crate::style`] (`layout_prop_call`).

/// Sentinel constructor for tags that have no constructor because they reference an existing in-scope variable rather than building a widget (e.g. `widget`).
pub const TAG_REFERENCES_VARIABLE: &str = "<in-scope variable>";

/// Built-in RSX tags paired with the Rust constructor path they transpile to.
///
/// Mirrors the tag dispatch in `ViewGen::emit_element`. Tags that share a constructor (e.g. `col`/`row`/`grid` -> `Container::new`) are listed once per spelling so lookups by tag name resolve every alias. A tag whose constructor is [`TAG_REFERENCES_VARIABLE`] emits no constructor: `widget` inlines the in-scope variable named by its content.
pub fn builtin_tags() -> &'static [(&'static str, &'static str)] {
    &[
        ("text", "Text::new"),
        ("heading", "Text::new"),
        ("section", "Container::new"),
        ("btn", "Button::new"),
        ("button", "Button::new"),
        ("col", "Container::new"),
        ("column", "Container::new"),
        ("row", "Container::new"),
        ("grid", "Container::new"),
        ("box", "StyledContainer::new"),
        ("img", "Image::new"),
        ("image", "Image::new"),
        ("svg", "Svg::new"),
        ("scroll", "LayoutScrollArea::new"),
        ("canvas", "Canvas::new"),
        ("widget", TAG_REFERENCES_VARIABLE),
    ]
}

/// Layout attribute keys common to every container-like tag.
///
/// Mirrors the recognized `match` arms in `style::layout_prop_call` that map to `LayoutStyle` builder calls, excluding the grid-only keys (`cols`, `span`, `row_span`) which downstream tooling offers solely on the `grid` tag. Aliases (`pad`/`padding`) are listed individually so completion offers both.
pub fn layout_attr_keys() -> &'static [&'static str] {
    &[
        "width",
        "height",
        "min_width",
        "min_height",
        "max_width",
        "max_height",
        "basis",
        "padding",
        "pad",
        "padding_x",
        "pad_x",
        "padding_y",
        "pad_y",
        "gap",
        "gap_x",
        "gap_y",
        "grow",
        "shrink",
        "wrap",
        "self",
        "direction",
        "align",
        "justify",
    ]
}

/// Whether `tag` is a built-in tag (vs. a component referencing another `.rsx`).
pub fn is_builtin_tag(tag: &str) -> bool {
    builtin_tags().iter().any(|(name, _)| *name == tag)
}

/// Whether `word` is a `[view]` control-flow keyword (so it starts a block, not an element).
pub fn is_control_flow_keyword(word: &str) -> bool {
    matches!(word, "if" | "for" | "let" | "else")
}

/// Attribute keys whose value is a color (a hex/keyword literal or a `[style]`/theme reference). The single source of truth for color-aware tooling (swatches, hover, go-to-definition, completion).
pub fn color_attr_keys() -> &'static [&'static str] {
    &[
        "color",
        "fill",
        "stroke",
        "outline",
        "from",
        "to",
        "mid",
        "shadow_color",
        "tint",
    ]
}

/// Completion attribute keys for `tag`: the shared layout keys plus the tag's own visual/behavioral keys. Mirrors the per-tag attribute handling in [`crate::view`]; a component tag (not built-in) takes its `Props` fields, so it returns no suggestions here.
pub fn tag_attr_keys(tag: &str) -> Vec<&'static str> {
    let with = |extra: &[&'static str]| {
        let mut keys = layout_attr_keys().to_vec();
        keys.extend_from_slice(extra);
        keys
    };
    match tag {
        // `transition:` animates a paint/color property (see `transition::parse_transition_value`), so it is offered on the tags whose codegen wires it: `text` (color), `box`/containers (fill/stroke/opacity).
        "text" | "heading" => vec!["size", "color", "lines", "transition"],
        "widget" => vec![],
        "btn" | "button" => with(&["on_press", "fill", "outline"]),
        "grid" => with(&["cols", "span", "row_span", "transition"]),
        "col" | "row" | "column" => with(&["fill", "stroke", "radius", "opacity", "transition"]),
        "box" | "section" => with(&[
            "fill",
            "stroke",
            "radius",
            "shadow_x",
            "shadow_y",
            "shadow_blur",
            "shadow_color",
            "from",
            "to",
            "mid",
            "opacity",
            "transition",
        ]),
        "img" | "image" => with(&["src"]),
        "svg" => with(&["src", "tint"]),
        _ if is_builtin_tag(tag) => layout_attr_keys().to_vec(),
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_and_control_flow_classification() {
        assert!(is_builtin_tag("col") && is_builtin_tag("text"));
        assert!(!is_builtin_tag("feature_card"));
        assert!(is_control_flow_keyword("for") && !is_control_flow_keyword("col"));
    }

    #[test]
    fn tag_attr_keys_layer_layout_and_tag_specific() {
        // A built-in container gets the shared layout keys.
        assert!(tag_attr_keys("col").contains(&"gap"));
        // The button adds its own keys on top of layout.
        let btn = tag_attr_keys("btn");
        assert!(btn.contains(&"on_press") && btn.contains(&"gap"));
        // `img` exposes `src`; a component (non-builtin) takes Props, so no suggestions here.
        assert!(tag_attr_keys("img").contains(&"src"));
        // `svg` exposes `src` (required) and `tint` (optional).
        let svg = tag_attr_keys("svg");
        assert!(svg.contains(&"src") && svg.contains(&"tint") && svg.contains(&"gap"));
        assert!(tag_attr_keys("feature_card").is_empty());
        // `transition:` is offered on the tags whose codegen wires it.
        assert!(tag_attr_keys("box").contains(&"transition"));
        assert!(tag_attr_keys("text").contains(&"transition"));
        assert!(tag_attr_keys("col").contains(&"transition"));
    }

    #[test]
    fn color_keys_cover_paint_and_gradient_attrs() {
        for key in [
            "color",
            "fill",
            "stroke",
            "outline",
            "from",
            "to",
            "mid",
            "shadow_color",
            "tint",
        ] {
            assert!(color_attr_keys().contains(&key), "missing {key}");
        }
    }
}
