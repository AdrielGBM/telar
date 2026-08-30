//! Authoritative registry of built-in RSX tags and layout attribute keys.
//!
//! These tables are the single source of truth shared between the transpiler's codegen (`view.rs`, `style.rs`) and downstream tooling such as `telar-analyzer` (completions, hover, go-to-definition). Keep them in sync with the `match` arms in [`crate::view`] (`emit_element`) and [`crate::style`] (`layout_prop_call`).

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
        ("canvas", "Canvas::new"),
        ("scroll", "LayoutScrollArea::new"),
        ("overlay", "Overlay::new"),
        ("lazy", "Lazy::new"),
        ("children", TAG_SLOT_PLACEHOLDER),
    ]
}

/// Layout attribute keys common to every container-like tag.
///
/// Mirrors the recognized `match` arms in `style::layout_prop_call` that map to `LayoutStyle` builder calls, excluding the grid-only keys (`cols`, `span`, `row_span`) which downstream tooling offers solely on the `grid` tag. Aliases (`pad`/`padding`) are listed individually so completion offers both.
pub fn layout_attr_keys() -> &'static [&'static str] {
    &[
        "track_rect",
        // Cuts a node's rendered output to its own laid-out rect: bare for both axes, `clip:x` or `clip:y` for one.
        "clip",
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
        "inset_top",
        "inset_bottom",
        "absolute",
        "gap",
        "gap_x",
        "gap_y",
        "grow",
        "shrink",
        "wrap",
        "cursor",
        "self",
        "axis",
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
    &["color", "fill", "stroke", "outline", "shadow_color"]
}

/// Named color keywords `color_expr` resolves (see `view/interp.rs`), alongside hex literals and `[style]`/
/// theme/`$signal` references. The single source of truth so completion, hover and swatch tooling agree.
pub fn color_keywords() -> &'static [&'static str] {
    &["transparent"]
}

/// The RGBA the one keyword colour resolves to, matching the `Color::TRANSPARENT` constant `color_expr`
/// emits (see `view/interp.rs`). `None` for anything outside [`color_keywords`].
///
/// It is kept because it says something no literal can: `#00000000` reads as opaque black at a glance.
/// `white` and `black` went with the rest of a palette the language never had — there was no `red` — and in a
/// themed toolkit a literal `white` is the mistake the theme exists to prevent: the value wanted is `surface`
/// or `ink`. An application with a palette of its own writes it as `[style]` constants.
pub fn keyword_color_rgba(name: &str) -> Option<[u8; 4]> {
    match name {
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
    // Per-edge names for the two properties that have one number per edge. Both also take the CSS shorthand
    // on their base key (`stroke_width:"0 0 1 0"`, `radius:"8 8 0 0"`); these are for naming one edge, and for
    // `start`/`end`, which is the only form that can follow the writing direction.
    "stroke_top",
    "stroke_right",
    "stroke_bottom",
    "stroke_left",
    "stroke_x",
    "stroke_y",
    "stroke_start",
    "stroke_end",
    "radius_top",
    "radius_bottom",
    "radius_left",
    "radius_right",
    "radius_top_left",
    "radius_top_right",
    "radius_bottom_right",
    "radius_bottom_left",
    "radius_start",
    "radius_end",
    "shadow_y",
    "shadow_blur",
    "shadow_color",
    "opacity",
    "on_press",
    // The non-primary half of `on_press`. Separate because a right- or middle-click otherwise falls through to whatever is behind the box, and folding it into `on_press` would make every pressable box swallow both.
    "on_alt_press",
    "on_long_press",
    "on_hover",
    // The continuous half of `on_hover`: where the pointer is, not just whether it is in.
    "on_pointer_move",
    "on_key",
    "on_drag",
    // A drag the markup can start but never finish is half a gesture: the release is where a pull-to-open, a
    // scrub or a reorder actually commits.
    "on_drag_end",
    "on_scroll",
    "on_focus",
    "cursor",
    // Which other buttons may start this box's drag; the primary one always can.
    "drag_button",
    // How far a press must travel before it is a drag and not a click on what sits under it.
    "drag_threshold",
    // A box that is drawn over something without standing between it and the pointer.
    "click_through",
    // A control inside something draggable, saying the stroke that starts on it is its own.
    "holds_stroke",
    "hover_style",
    "active_style",
    // Not a paint prop: the framework reads it to close the pointer, the hover tracking and the cursor, which is what stops each author re-deriving a different subset by hand.
    "disabled",
    "disabled_style",
    // The focus ring. Composed over whichever state won rather than replacing it, so it survives a hover.
    "focus_style",
    "transition",
];

/// The colour names `ThemeTokens` defines, which a bare `color:`/`fill:`/`stroke:`/`tint:` value resolves
/// through the **trait method** rather than through a field of the same name.
///
/// Because the trait is the contract: `set_theme` requires it, and every catalogue component reads its
/// colours from it. A theme shaped like shadcn's, for instance, calls its quiet *surface* `muted` and its
/// quiet *ink* `muted-foreground`, and maps `ThemeTokens::muted()` to the latter — so a `.rsx` reading the field got a
/// background where the catalogue beside it got an ink, and the text came out invisible with nothing to
/// diagnose. A theme's own tokens (`card`, `accent`, `popover`) are not in the trait and stay field reads,
/// and `theme.name` remains the escape hatch for a field this list shadows.
pub const THEME_COLOR_TOKENS: &[&str] = &[
    "primary",
    "on_primary",
    "muted",
    "scrollbar",
    "ink",
    "surface",
    "surface_alt",
    "border",
    "success",
    "warning",
    "error",
    "info",
    "highlight_low",
    "highlight_med",
    "highlight_high",
];

/// `align:` on a container: where children sit across the axis they are not laid along.
pub const ALIGN_VALUES: &[(&str, &str)] = &[
    ("center", "AlignItems::CENTER"),
    ("start", "AlignItems::START"),
    ("end", "AlignItems::END"),
    ("stretch", "AlignItems::STRETCH"),
    ("flex-start", "AlignItems::FLEX_START"),
    ("flex-end", "AlignItems::FLEX_END"),
];

/// `justify:` on a container: how the free space along the layout axis is distributed.
pub const JUSTIFY_VALUES: &[(&str, &str)] = &[
    ("center", "JustifyContent::CENTER"),
    ("start", "JustifyContent::START"),
    ("end", "JustifyContent::END"),
    ("between", "JustifyContent::SPACE_BETWEEN"),
    ("space-between", "JustifyContent::SPACE_BETWEEN"),
    ("around", "JustifyContent::SPACE_AROUND"),
    ("space-around", "JustifyContent::SPACE_AROUND"),
    ("evenly", "JustifyContent::SPACE_EVENLY"),
    ("space-evenly", "JustifyContent::SPACE_EVENLY"),
];

/// `self:` — one child's override of its parent's `align:`. Paired with the `LayoutStyle` builder it calls.
pub const SELF_VALUES: &[(&str, &str)] = &[
    ("stretch", "align_self_stretch"),
    ("center", "align_self_center"),
    ("start", "align_self_start"),
    ("end", "align_self_end"),
];

/// `axis:` — which way a container lays its children out. `row_reverse` is reversed in both writing
/// directions, unlike `row`, which follows the active one.
///
/// Named for the axis rather than the direction because `direction` is the *writing* direction everywhere
/// else, and this attribute had taken the word for the wrong property. The axis is already implied by the
/// `col`/`row`/`grid` tag; this exists only to override it.
pub const AXIS_VALUES: &[(&str, &str)] = &[
    ("col", "flex_column"),
    ("column", "flex_column"),
    ("row", "flex_row"),
    ("row_reverse", "flex_row_reverse"),
];

/// `absolute` — out of flow, pinned by the insets the author names; `absolute:fill` is the all-four-at-zero
/// shorthand. The empty spelling is the bare flag.
pub const ABSOLUTE_VALUES: &[(&str, &str)] = &[("", "absolute"), ("fill", "absolute_fill")];

/// `wrap` — a flag, spelled bare or as its own name.
pub const WRAP_VALUES: &[(&str, &str)] = &[("", "flex_wrap")];

/// A key that is the assertion itself: writing it turns the thing on and leaving it out leaves it off.
///
/// `:true` and `:false` were exact synonyms of a shorter spelling and of no spelling at all, so a value here
/// is a mistake now rather than a third way of saying one of two things.
pub const FLAG_VALUES: &[(&str, &str)] = &[("", "true")];

/// `fit:` on `img`/`svg` (CSS `object-fit`): how the picture is scaled into the box it was given.
pub const FIT_VALUES: &[(&str, &str)] = &[
    ("contain", "ObjectFit::Contain"),
    ("fill", "ObjectFit::Fill"),
    ("cover", "ObjectFit::Cover"),
    ("contain_integer", "ObjectFit::ContainInteger"),
];

/// `raster:` — how samples meet the pixel grid, for a glyph and for a picture alike.
///
/// One key now, where `text` said `raster:` and `img` said `filter:` for the same decision in two
/// vocabularies. `linear`/`nearest` remain as the picture's own words for the two ends.
pub const RASTER_VALUES: &[(&str, &str)] = &[
    ("smooth", "Raster::Smooth"),
    ("subpixel", "Raster::Smooth"),
    ("linear", "Raster::Smooth"),
    ("pixel", "Raster::Pixel"),
    ("nearest", "Raster::Pixel"),
];

/// `font_weight:` — the OpenType weight axis, named.
///
/// One spelling per step, where there were sixteen for nine values: `semibold`, `semi-bold` and `demibold`
/// were one weight, and a synonym is cost with nothing bought. `heavy` for 900 rather than CSS's `black`,
/// because `font_weight` is writable on a container now and `col font_weight:black color:black` would be two
/// meanings of one word on one line.
pub const FONT_WEIGHT_VALUES: &[(&str, &str)] = &[
    ("thin", "100"),
    ("extralight", "200"),
    ("light", "300"),
    ("normal", "400"),
    ("medium", "500"),
    ("semibold", "600"),
    ("bold", "700"),
    ("extrabold", "800"),
    ("heavy", "900"),
];

/// `font_style:` — the slant of the face.
///
/// Three-valued where the markup had a bare `italic` flag, because the shaper has modelled oblique all
/// along and nothing could ask for it.
pub const FONT_STYLE_VALUES: &[(&str, &str)] = &[
    ("normal", "FontStyle::Normal"),
    ("italic", "FontStyle::Italic"),
    ("oblique", "FontStyle::Oblique"),
];

/// `text_wrap:` — whether text wraps into its box or keeps one line.
///
/// Named apart from the container's `wrap:`, which is flex-wrap and one character away from the `nowrap`
/// flag this replaces.
pub const TEXT_WRAP_VALUES: &[(&str, &str)] = &[
    ("wrap", "TextWrap::Wrap"),
    ("nowrap", "TextWrap::NoWrap"),
    ("no_wrap", "TextWrap::NoWrap"),
];

/// `text_align:` — where the lines sit inside the text's own box, as against a container's `align:`, which
/// places the box among its siblings.
pub const TEXT_ALIGN_VALUES: &[(&str, &str)] = &[
    ("start", "TextAlign::Start"),
    ("left", "TextAlign::Start"),
    ("center", "TextAlign::Center"),
    ("centre", "TextAlign::Center"),
    ("end", "TextAlign::End"),
    ("right", "TextAlign::End"),
    ("justify", "TextAlign::Justify"),
    ("justified", "TextAlign::Justify"),
];

/// What an attribute's value has to be for its key to mean anything, so a value outside it is a build error
/// on the attribute instead of a property quietly dropped or quietly defaulted.
///
/// The counterpart to [`tag_attr_keys`]: that answers which keys a tag has, this answers what those keys
/// take. A key absent from [`value_kind`] carries a value only rustc can judge — a string, a callback, an
/// expression — and is left alone.
pub enum ValueKind {
    /// A closed set of spellings, each paired with the Rust name it generates. Also the completion list.
    Keywords(&'static [(&'static str, &'static str)]),
    /// A closed set of spellings *or* a plain number, for the one axis that is genuinely both: an OpenType
    /// weight *is* a number, and the names are the nine steps of it everyone actually writes.
    KeywordsOrNumber(&'static [(&'static str, &'static str)]),
    /// A number: a literal, a `$signal`, a `theme.…` read, a `[style]` constant, or a binding in scope.
    Number,
    /// A number that may also be a percentage of the containing block.
    Dimension,
    /// One number per edge: a single value, or the CSS 2/3/4-value shorthand.
    Edges,
    /// A colour: a hex literal, `transparent`, a `theme.…` read, a `[style]` constant, a `$signal`, or an
    /// expression that yields one.
    Color,
}

/// The value schema of `tag`'s `key`, or `None` when the key takes a free-form value.
///
/// Tag-aware because one name is still two properties: `stroke` is a colour on a box and a *width* on an
/// `svg`, and the four edges a box collects are one plain number on a `path`.
pub fn value_kind(tag: &str, key: &str) -> Option<ValueKind> {
    match key {
        "text_align" => return Some(ValueKind::Keywords(TEXT_ALIGN_VALUES)),
        "text_wrap" => return Some(ValueKind::Keywords(TEXT_WRAP_VALUES)),
        "font_style" => return Some(ValueKind::Keywords(FONT_STYLE_VALUES)),
        "ellipsis" | "click_through" | "holds_stroke" | "secret" => {
            return Some(ValueKind::Keywords(FLAG_VALUES));
        }
        "font_weight" => return Some(ValueKind::KeywordsOrNumber(FONT_WEIGHT_VALUES)),
        // A stroke *width* on an `svg`, where every other tag means a colour by the same name; a `path`'s
        // fill rule is a keyword, not a paint.
        _ if key != "stroke" && color_attr_keys().contains(&key) => return Some(ValueKind::Color),
        "stroke" if tag != "svg" && tag != "path" => return Some(ValueKind::Color),
        "align" => return Some(ValueKind::Keywords(ALIGN_VALUES)),
        "justify" => return Some(ValueKind::Keywords(JUSTIFY_VALUES)),
        "self" => return Some(ValueKind::Keywords(SELF_VALUES)),
        "axis" => return Some(ValueKind::Keywords(AXIS_VALUES)),
        "absolute" => return Some(ValueKind::Keywords(ABSOLUTE_VALUES)),
        "wrap" => return Some(ValueKind::Keywords(WRAP_VALUES)),
        "fit" => return Some(ValueKind::Keywords(FIT_VALUES)),
        "raster" => return Some(ValueKind::Keywords(RASTER_VALUES)),
        // Every length resolves against the containing block where CSS says it does: sizes, and now the
        // edges and gaps too. `%` used to work on six size keys and nowhere else, so `padding:50%` was a
        // fact about which helper the emitter happened to call.
        "width" | "height" | "min_width" | "min_height" | "max_width" | "max_height" | "basis"
        | "flex_basis" | "padding" | "pad" | "padding_x" | "pad_x" | "padding_y" | "pad_y"
        | "padding_start" | "pad_start" | "padding_end" | "pad_end" | "margin_start"
        | "margin_end" | "inset_start" | "inset_end" | "inset_top" | "inset_bottom" | "gap"
        | "gap_x" | "gap_y" => {
            return Some(ValueKind::Dimension);
        }
        // Ratios, not lengths: half of nothing is not a meaning either of them has.
        "aspect" | "aspect_ratio" | "grow" | "shrink" => return Some(ValueKind::Number),
        "font_size" => return Some(ValueKind::Number),
        // A stroke *width* on an `svg`, where every other tag means a colour by the same name.
        "stroke" if tag == "svg" => return Some(ValueKind::Number),
        _ => {}
    }
    // A `path`'s stroke is one plain width, not the four `crate::edges` collects for a box.
    let edged = key == "radius"
        || key.strip_prefix("radius_").is_some_and(corner_suffix)
        || (tag != "path"
            && (key == "stroke_width" || key.strip_prefix("stroke_").is_some_and(side_suffix)));
    edged.then_some(ValueKind::Edges)
}

/// Whether `suffix` names an edge of `radius_*`. Mirrors `crate::edges::corner_target`.
fn corner_suffix(suffix: &str) -> bool {
    matches!(
        suffix,
        "top"
            | "bottom"
            | "left"
            | "right"
            | "top_left"
            | "top_right"
            | "bottom_right"
            | "bottom_left"
            | "start"
            | "end"
    )
}

/// Whether `suffix` names a side of `stroke_*`. Mirrors `crate::edges::side_target`.
fn side_suffix(suffix: &str) -> bool {
    matches!(
        suffix,
        "top" | "right" | "bottom" | "left" | "x" | "y" | "start" | "end"
    )
}

/// The Rust name a keyword spelling generates, or `None` when the set does not contain it.
pub fn keyword(
    table: &'static [(&'static str, &'static str)],
    value: &str,
) -> Option<&'static str> {
    table
        .iter()
        .find(|(name, _)| *name == value)
        .map(|(_, rust)| *rust)
}

/// Declarative affine transform attribute keys (see `container::transform_call`, which gates
/// `.with_transform` emission on this exact list — shared here so codegen and completion cannot drift).
/// The text properties that flow down the tree, so a container may name any of them for everything beneath
/// it and a leaf takes what it did not name itself.
///
/// The same set on a container and on a `text`, which is the point: `font_size:11` means one thing, and
/// where it is written decides how far it reaches rather than what it says. Absent from it are the
/// properties that clamp *one* paragraph — `lines`, `ellipsis` — which would be nonsense applied to a
/// subtree, and which [`renderer_core::Declared`] has no way to spell for the same reason.
pub const INHERITABLE_TEXT_KEYS: &[&str] = &[
    "font_size",
    "font_family",
    "font_weight",
    "font_style",
    "color",
    "text_align",
    "text_wrap",
    "line_height",
    "letter_spacing",
    "raster",
];

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
        // `transition(…)` animates a paint/color property (see `transition::parse_transition_value`), so it is offered on the tags whose codegen wires it: `text` (color), `box`/containers (fill/stroke/opacity).
        // A `text` is a leaf, but it is a leaf *in a flex box*: it takes the layout keys that size and place it
        // among its siblings alongside its own type keys.
        "text" => with(&[
            "font_size",
            "font_family",
            "color",
            "font_weight",
            "font_style",
            "text_align",
            "lines",
            "ellipsis",
            "text_wrap",
            "line_height",
            "letter_spacing",
            "raster",
            "transition",
        ]),
        // Both splice a whole widget in, so neither takes layout or paint keys: the expression owns its style.
        // The `children` slot placeholder takes an optional `name:` for a named slot, and `in:` for the context
        // a compound component builds them inside.
        "children" => vec!["name", "in"],
        // box/col/row/grid share one paint+behavior set (the codegen treats them identically); grid adds its track keys.
        "grid" => {
            let mut keys = with(CONTAINER_PAINT);
            keys.extend_from_slice(TRANSFORM_ATTR_KEYS);
            keys.extend_from_slice(INHERITABLE_TEXT_KEYS);
            keys.extend_from_slice(&["cols", "span", "row_span"]);
            keys
        }
        "col" | "row" | "box" => {
            let mut keys = with(CONTAINER_PAINT);
            keys.extend_from_slice(TRANSFORM_ATTR_KEYS);
            keys.extend_from_slice(INHERITABLE_TEXT_KEYS);
            // A box is a grid *item* wherever its parent is a grid, so it carries the placement keys even
            // though it is not itself one.
            keys.extend_from_slice(&["span", "row_span"]);
            keys
        }
        // `radius` rounds the picture itself, in every form a `box` takes it; a leaf takes no other paint key.
        "img" | "image" => {
            let mut keys = with(&["src", "fit", "raster", "radius"]);
            keys.extend(CONTAINER_PAINT.iter().filter(|k| k.starts_with("radius_")));
            keys
        }
        // `keep:` names the surface-kept position of this viewport, so a remounted tree reopens where it was.
        "scroll" => with(&["keep"]),
        // `paint` is the drawing itself; everything else shapes the leaf it draws into.
        "canvas" => with(&["paint"]),
        // `input` binds `value:$signal` and takes text-style keys plus an optional Enter handler.
        "input" => with(&[
            "value",
            "font_size",
            "font_family",
            "color",
            "placeholder",
            "on_submit",
            "secret",
        ]),
        "svg" => with(&["src", "color", "stroke", "fit"]),
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
        assert!(svg.contains(&"src") && svg.contains(&"color") && svg.contains(&"gap"));
        assert!(tag_attr_keys("feature_card").is_empty());
        // `transition(…)` is offered on the tags whose codegen wires it.
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
        assert_eq!(color_keywords(), &["transparent"]);
        assert_eq!(keyword_color_rgba("transparent"), Some([0, 0, 0, 0]));
        assert_eq!(keyword_color_rgba("cerulean"), None);
        // The palette the language never had: three named colours with no `red` among them, and in a themed
        // toolkit the two that went are the ones a theme exists to supply.
        assert_eq!(keyword_color_rgba("white"), None);
        assert_eq!(keyword_color_rgba("black"), None);
    }

    #[test]
    fn color_keys_cover_every_attribute_that_paints() {
        for key in ["color", "fill", "stroke", "outline", "shadow_color"] {
            assert!(color_attr_keys().contains(&key), "missing {key}");
        }
        // A gradient's stops are a value now, so its old keys are not properties a box has.
        for key in ["gradient", "from", "to", "mid", "mid_pos", "radial_radius"] {
            assert!(!color_attr_keys().contains(&key), "{key} should be gone");
        }
    }
}
