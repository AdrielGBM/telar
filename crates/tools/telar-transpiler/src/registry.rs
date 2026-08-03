//! Authoritative registry of built-in RSX tags and layout attribute keys.
//!
//! These tables are the single source of truth shared between the transpiler's codegen (`view.rs`, `style.rs`) and downstream tooling such as `telar-analyzer` (completions, hover, go-to-definition). Keep them in sync with the `match` arms in [`crate::view`] (`emit_element`) and [`crate::style`] (`layout_prop_call`).

/// Sentinel constructor for tags that have no constructor because they reference an existing in-scope variable rather than building a widget (e.g. `widget`).
pub const TAG_REFERENCES_VARIABLE: &str = "<in-scope variable>";

/// Sentinel constructor for tags whose content is a Rust expression evaluated at each construction point rather than a widget built once (e.g. `build`).
pub const TAG_BUILDS_EXPRESSION: &str = "<expression>";

/// Sentinel constructor for the `children` slot placeholder, which builds no widget: it splices the
/// caller-supplied children (from the component's `Slots` argument) into the enclosing container.
pub const TAG_SLOT_PLACEHOLDER: &str = "<slot placeholder>";

/// Built-in RSX tags paired with the Rust constructor path they transpile to.
///
/// Mirrors the tag dispatch in `ViewGen::emit_element`. Tags that share a constructor (e.g. `col`/`row`/`grid` -> `Container::new`) are listed once per spelling so lookups by tag name resolve every alias. A tag whose constructor is [`TAG_REFERENCES_VARIABLE`] emits no constructor: `widget` inlines the in-scope variable named by its content.
pub fn builtin_tags() -> &'static [(&'static str, &'static str)] {
    &[
        ("text", "Text::new"),
        ("col", "Container::new"),
        ("row", "Container::new"),
        ("grid", "Container::new"),
        ("box", "StyledContainer::new"),
        ("img", "Image::new"),
        ("image", "Image::new"),
        ("input", "Input::new"),
        ("svg", "Svg::new"),
        ("path", "Path::new"),
        ("scroll", "LayoutScrollArea::new"),
        ("canvas", "Canvas::new"),
        ("overlay", "Overlay::new"),
        ("lazy", "Lazy::new"),
        ("widget", TAG_REFERENCES_VARIABLE),
        ("build", TAG_BUILDS_EXPRESSION),
        ("children", TAG_SLOT_PLACEHOLDER),
    ]
}

/// Layout attribute keys common to every container-like tag.
///
/// Mirrors the recognized `match` arms in `style::layout_prop_call` that map to `LayoutStyle` builder calls, excluding the grid-only keys (`cols`, `span`, `row_span`) which downstream tooling offers solely on the `grid` tag. Aliases (`pad`/`padding`) are listed individually so completion offers both.
pub fn layout_attr_keys() -> &'static [&'static str] {
    &[
        "track_rect",
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
        "padding_start",
        "pad_start",
        "padding_end",
        "pad_end",
        "margin_start",
        "margin_end",
        "inset_start",
        "inset_end",
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

/// Named color keywords `color_expr` resolves (see `view/interp.rs`), alongside hex literals and `[style]`/
/// theme/`$signal` references. The single source of truth so completion, hover and swatch tooling agree.
pub fn color_keywords() -> &'static [&'static str] {
    &["white", "black", "transparent"]
}

/// The RGBA a keyword color resolves to, matching the `Color::WHITE`/`BLACK`/`TRANSPARENT` constants
/// `color_expr` emits (see `view/interp.rs`). `None` for anything outside [`color_keywords`].
pub fn keyword_color_rgba(name: &str) -> Option<[u8; 4]> {
    match name {
        "white" => Some([255, 255, 255, 255]),
        "black" => Some([0, 0, 0, 255]),
        "transparent" => Some([0, 0, 0, 0]),
        _ => None,
    }
}

/// The full paint + behavior attribute set every styled container (`box`, `col`, `row`, `grid`) accepts.
/// Kept in one place so the four tags stay consistent — the codegen already treats them identically
/// (`rect_style_pieces` resolves fill/stroke/shadow/gradient/opacity for all of them, and `on_press`
/// is wired on both `Container` and `StyledContainer`).
///
/// No generic value-callback key (`on_change` et al.) is in this list: a container has no "value" to
/// change, so a container-level callback here would be meaningless (the codegen has nothing to call it
/// on `Container`/`StyledContainer`). Instead each value-bearing widget (built as a component) declares
/// its own callback as a `Props` field, named for what the value actually is: `on_toggle` for a bool
/// (checkbox/toggle), `on_select` for a picked index (radio/menu/select), `on_change` for a continuous
/// value (slider), `on_submit` for a commit (text_field, fires on Enter — it has no per-keystroke
/// callback). `emit_component_call` boxes any closure-valued attr generically by field name (see
/// `component_props_arg` in `view/component.rs`), so each of these works today with no transpiler change
/// needed here.
const CONTAINER_PAINT: &[&str] = &[
    "fill",
    "stroke",
    "stroke_width",
    "radius",
    "shadow_x",
    "shadow_y",
    "shadow_blur",
    "shadow_color",
    "gradient",
    "from",
    "to",
    "mid",
    "mid_pos",
    "radial_radius",
    "opacity",
    "on_press",
    "on_long_press",
    "on_hover",
    "on_key",
    "on_drag",
    // A drag the markup can start but never finish is half a gesture: the release is where a pull-to-open, a
    // scrub or a reorder actually commits.
    "on_drag_end",
    "on_scroll",
    "on_focus",
    "hover_style",
    "active_style",
    "transition",
];

/// Declarative affine transform attribute keys (see `container::transform_call`, which gates
/// `.with_transform` emission on this exact list — shared here so codegen and completion cannot drift).
pub const TRANSFORM_ATTR_KEYS: &[&str] = &[
    "rotate",
    "scale",
    "scale_x",
    "scale_y",
    "translate_x",
    "translate_y",
];

/// Completion attribute keys for `tag`: the shared layout keys plus the tag's own visual/behavioral keys. Mirrors the per-tag attribute handling in [`crate::view`]; a component tag (not built-in) takes its `Props` fields, so it returns no suggestions here.
pub fn tag_attr_keys(tag: &str) -> Vec<&'static str> {
    let with = |extra: &[&'static str]| {
        let mut keys = layout_attr_keys().to_vec();
        keys.extend_from_slice(extra);
        keys
    };
    match tag {
        // `transition:` animates a paint/color property (see `transition::parse_transition_value`), so it is offered on the tags whose codegen wires it: `text` (color), `box`/containers (fill/stroke/opacity).
        // A `text` is a leaf, but it is a leaf *in a flex box*: it takes the layout keys that size and place it
        // among its siblings alongside its own type keys.
        "text" => with(&[
            "size",
            "color",
            "weight",
            "italic",
            "align",
            "lines",
            "ellipsis",
            "line_height",
            "letter_spacing",
            "transition",
        ]),
        // Both splice a whole widget in, so neither takes layout or paint keys: the expression owns its style.
        "widget" | "build" => vec![],
        // The `children` slot placeholder takes only an optional `name:` for a named slot.
        "children" => vec!["name"],
        // box/col/row/grid share one paint+behavior set (the codegen treats them identically); grid adds its track keys.
        "grid" => {
            let mut keys = with(CONTAINER_PAINT);
            keys.extend_from_slice(TRANSFORM_ATTR_KEYS);
            keys.extend_from_slice(&["cols", "span", "row_span"]);
            keys
        }
        "col" | "row" | "box" => {
            let mut keys = with(CONTAINER_PAINT);
            keys.extend_from_slice(TRANSFORM_ATTR_KEYS);
            // A box is a grid *item* wherever its parent is a grid, so it carries the placement keys even
            // though it is not itself one.
            keys.extend_from_slice(&["span", "row_span"]);
            keys
        }
        // `radius` rounds the picture itself; a leaf takes no other paint key.
        "img" | "image" => with(&["src", "fit", "filter", "radius"]),
        // `keep:` names the surface-kept position of this viewport, so a remounted tree reopens where it was.
        "scroll" => with(&["keep"]),
        // `input` binds `value:$signal` and takes text-style keys plus an optional Enter handler.
        "input" => with(&["value", "size", "color", "on_submit"]),
        "svg" => with(&["src", "tint", "stroke", "fit"]),
        // `lazy` holds its subtree back until `when:` is true.
        "lazy" => with(&["when"]),
        // `path` draws SVG path-data (`d:`) with a solid fill/stroke; sized by width/height like a leaf.
        "path" => with(&["d", "fill", "stroke", "stroke_width", "fill_rule"]),
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
        // `btn`/`heading`/`section` are no longer built-in tags: they resolve as widget components.
        assert!(!is_builtin_tag("btn") && !is_builtin_tag("heading") && !is_builtin_tag("section"));
        assert!(is_control_flow_keyword("for") && !is_control_flow_keyword("col"));
    }

    #[test]
    fn tag_attr_keys_layer_layout_and_tag_specific() {
        // A built-in container gets the shared layout keys.
        assert!(tag_attr_keys("col").contains(&"gap"));
        // `btn` is now a component (not built-in): its attributes come from its Props, so no builtin keys here.
        assert!(tag_attr_keys("btn").is_empty());
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
        // Transform keys are appended from `TRANSFORM_ATTR_KEYS`, not inlined into `CONTAINER_PAINT`.
        for tag in ["box", "col", "row", "grid"] {
            for key in TRANSFORM_ATTR_KEYS {
                assert!(tag_attr_keys(tag).contains(key), "{tag} missing {key}");
            }
        }
    }

    #[test]
    fn color_keywords_match_keyword_color_rgba() {
        assert_eq!(color_keywords(), &["white", "black", "transparent"]);
        assert_eq!(keyword_color_rgba("white"), Some([255, 255, 255, 255]));
        assert_eq!(keyword_color_rgba("black"), Some([0, 0, 0, 255]));
        assert_eq!(keyword_color_rgba("transparent"), Some([0, 0, 0, 0]));
        assert_eq!(keyword_color_rgba("cerulean"), None);
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
