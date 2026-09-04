//! Authoritative registry of built-in RSX tags and layout attribute keys.
//!
//! These tables are the single source of truth shared between the transpiler's codegen (`view.rs`, `style.rs`) and downstream tooling such as `telar-analyzer` (completions, hover, go-to-definition). Keep them in sync with the `match` arms in [`crate::view`] (`emit_element`) and [`crate::style`] (`layout_prop_call`).

/// Sentinel constructor for the `children` slot placeholder, which builds no widget: it splices the caller-supplied children (from the component's `Slots` argument) into the enclosing container.
pub const TAG_SLOT_PLACEHOLDER: &str = "<slot placeholder>";

/// Built-in RSX tags paired with the Rust constructor path they transpile to.
///
/// Mirrors the tag dispatch in `ViewGen::emit_element`. Tags that share a constructor (e.g. `col`/`row`/`grid` -> `Container::new`) are listed once per spelling so lookups by tag name resolve every alias. Every tag here builds something; the one exception carries `TAG_SLOT_PLACEHOLDER` and says so.
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

/// Layout attribute keys common to every container-like tag, paired with what each one takes.
///
/// Mirrors the recognized `match` arms in `style::layout_prop_call` that map to `LayoutStyle` builder calls, excluding the grid-only keys (`cols`, `span`, `row_span`) which downstream tooling offers solely on the `grid` tag. Aliases (`pad`/`padding`) are listed individually so completion offers both.
const LAYOUT_ATTRS: &[AttrSpec] = &[
    AttrSpec::free("track_rect"),
    AttrSpec::free("clip").doc(
        "Cuts a node's output to its laid-out rect: bare, or a `Clip` shape naming axis, radius and inset.",
    ),
    AttrSpec::num("width"),
    AttrSpec::num("height"),
    AttrSpec::num("min_width"),
    AttrSpec::num("min_height"),
    AttrSpec::num("max_width"),
    AttrSpec::num("max_height"),
    AttrSpec::num("basis"),
    AttrSpec::num("flex_basis"),
    AttrSpec::num("aspect"),
    AttrSpec::num("aspect_ratio"),
    AttrSpec::num("padding"),
    AttrSpec::num("pad"),
    AttrSpec::num("padding_x"),
    AttrSpec::num("pad_x"),
    AttrSpec::num("padding_y"),
    AttrSpec::num("pad_y"),
    AttrSpec::num("padding_start"),
    AttrSpec::num("pad_start"),
    AttrSpec::num("padding_end"),
    AttrSpec::num("pad_end"),
    AttrSpec::num("margin_start"),
    AttrSpec::num("margin_end"),
    AttrSpec::num("inset_start"),
    AttrSpec::num("inset_end"),
    AttrSpec::num("inset_top"),
    AttrSpec::num("inset_bottom"),
    AttrSpec::keywords("absolute", ABSOLUTE_VALUES),
    AttrSpec::boolean("shown").doc(
        "Whether the node is in flow, re-resolved from what it reads — unlike `display:none`, which could not undo itself.",
    ),
    AttrSpec::num("gap"),
    AttrSpec::num("gap_x"),
    AttrSpec::num("gap_y"),
    AttrSpec::num("grow"),
    AttrSpec::num("shrink"),
    AttrSpec::keywords("wrap", WRAP_VALUES),
    AttrSpec::free("cursor"),
    AttrSpec::keywords("self", SELF_VALUES),
    AttrSpec::keywords("axis", AXIS_VALUES),
    AttrSpec::keywords("align", ALIGN_VALUES),
    AttrSpec::keywords("justify", JUSTIFY_VALUES),
];

/// The layout keys, spelling only. Completion and the emitter's unknown-attribute check read the same table [`value_kind`] does.
pub fn layout_attr_keys() -> Vec<&'static str> {
    LAYOUT_ATTRS.iter().map(|spec| spec.key).collect()
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

/// Named color keywords `color_expr` resolves (see `view/interp.rs`), alongside hex literals and `[style]`/ theme/`$signal` references. The single source of truth so completion, hover and swatch tooling agree.
pub fn color_keywords() -> &'static [&'static str] {
    &["transparent"]
}

/// The RGBA the one keyword colour resolves to, matching the `Color::TRANSPARENT` constant `color_expr` emits (see `view/interp.rs`). `None` for anything outside [`color_keywords`].
///
/// It is kept because it says something no literal can: `#00000000` reads as opaque black at a glance. `white` and `black` went with the rest of a palette the language never had — there was no `red` — and in a themed toolkit a literal `white` is the mistake the theme exists to prevent: the value wanted is `surface` or `ink`. An application with a palette of its own declares it on its theme type.
pub fn keyword_color_rgba(name: &str) -> Option<[u8; 4]> {
    match name {
        "transparent" => Some([0, 0, 0, 0]),
        _ => None,
    }
}

/// The full paint + behavior attribute set every styled container (`box`, `col`, `row`, `grid`) accepts. Kept in one place so the four tags stay consistent — the codegen already treats them identically (`rect_style_pieces` resolves fill/stroke/shadow/gradient/opacity for all of them, and `on_press` is wired on both `Container` and `StyledContainer`).
///
/// No generic value-callback key (`on_change` et al.) is in this list: a container has no "value" to change, so a container-level callback here would be meaningless (the codegen has nothing to call it on `Container`/`StyledContainer`). Instead each value-bearing widget (built as a component) declares its own callback as a `Props` field, named for what the value actually is: `on_toggle` for a bool (checkbox/toggle), `on_select` for a picked index (radio/menu/select), `on_change` for a continuous value (slider), `on_submit` for a commit (text_field, fires on Enter — it has no per-keystroke callback). `emit_component_call` boxes any closure-valued attr generically by field name (see `component_props_arg` in `view/component.rs`), so each of these works today with no transpiler change needed here.
const CONTAINER_PAINT: &[AttrSpec] = &[
    AttrSpec::color("fill"),
    AttrSpec::color("stroke"),
    AttrSpec::edges("stroke_width"),
    AttrSpec::edges("radius"),
    AttrSpec::free("shadow_x"),
    AttrSpec::edges("stroke_top").doc(
        "One number per edge. `stroke_width` and `radius` also take the CSS shorthand (`radius:\"8 8 0 0\"`); these name a single edge, and `start`/`end`, the only form that follows the writing direction.",
    ),
    AttrSpec::edges("stroke_right"),
    AttrSpec::edges("stroke_bottom"),
    AttrSpec::edges("stroke_left"),
    AttrSpec::edges("stroke_x"),
    AttrSpec::edges("stroke_y"),
    AttrSpec::edges("stroke_start"),
    AttrSpec::edges("stroke_end"),
    AttrSpec::edges("radius_top"),
    AttrSpec::edges("radius_bottom"),
    AttrSpec::edges("radius_left"),
    AttrSpec::edges("radius_right"),
    AttrSpec::edges("radius_top_left"),
    AttrSpec::edges("radius_top_right"),
    AttrSpec::edges("radius_bottom_right"),
    AttrSpec::edges("radius_bottom_left"),
    AttrSpec::edges("radius_start"),
    AttrSpec::edges("radius_end"),
    AttrSpec::free("shadow_y"),
    AttrSpec::free("shadow_blur"),
    AttrSpec::color("shadow_color"),
    AttrSpec::free("opacity"),
    AttrSpec::free("on_press"),
    AttrSpec::free("on_alt_press").doc(
        "Separate from `on_press`, or every pressable box would swallow right- and middle-clicks too.",
    ),
    AttrSpec::free("on_long_press"),
    AttrSpec::free("on_hover"),
    AttrSpec::free("on_pointer_move"),
    AttrSpec::free("on_key"),
    AttrSpec::free("on_drag"),
    AttrSpec::free("on_drag_end"),
    AttrSpec::free("on_scroll"),
    AttrSpec::free("on_focus"),
    AttrSpec::free("cursor"),
    AttrSpec::free("drag_button")
        .doc("Which other buttons may start this box's drag; the primary one always can."),
    AttrSpec::num("drag_threshold")
        .doc("How far a press must travel before it is a drag rather than a click."),
    AttrSpec::keywords("role", ROLE_VALUES)
        .doc("What the box is, beyond a box: a region, a list, a heading."),
    AttrSpec::flag("click_through")
        .doc("Drawn over something without standing between it and the pointer."),
    AttrSpec::flag("holds_stroke")
        .doc("A control inside something draggable, claiming the stroke that starts on it."),
    AttrSpec::free("hover_style"),
    AttrSpec::free("active_style"),
    AttrSpec::free("disabled").doc(
        "Read by the framework to close the pointer, the hover tracking and the cursor together.",
    ),
    AttrSpec::free("disabled_style"),
    AttrSpec::free("focus_style").doc(
        "Composed over whichever state won rather than replacing it, so it survives a hover.",
    ),
    AttrSpec::free("transition"),
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

/// `axis:` — which way a container lays its children out. `row_reverse` is reversed in both writing directions, unlike `row`, which follows the active one.
///
/// Named for the axis rather than the direction because `direction` is the *writing* direction everywhere else, and this attribute had taken the word for the wrong property. The axis is already implied by the `col`/`row`/`grid` tag; this exists only to override it.
pub const AXIS_VALUES: &[(&str, &str)] = &[
    ("col", "flex_column"),
    ("column", "flex_column"),
    ("row", "flex_row"),
    ("row_reverse", "flex_row_reverse"),
];

/// `absolute` — out of flow, pinned by the insets the author names; `absolute:fill` is the all-four-at-zero shorthand. The empty spelling is the bare flag.
pub const ABSOLUTE_VALUES: &[(&str, &str)] = &[("", "absolute"), ("fill", "absolute_fill")];

/// `wrap` — a flag, spelled bare or as its own name.
pub const WRAP_VALUES: &[(&str, &str)] = &[("", "flex_wrap")];

/// A key that is the assertion itself: writing it turns the thing on and leaving it out leaves it off.
///
/// `:true` and `:false` were exact synonyms of a shorter spelling and of no spelling at all, so a value here is a mistake now rather than a third way of saying one of two things.
pub const FLAG_VALUES: &[(&str, &str)] = &[("", "true")];

/// `fit:` on `img`/`svg` (CSS `object-fit`): how the picture is scaled into the box it was given.
pub const FIT_VALUES: &[(&str, &str)] = &[
    ("contain", "ObjectFit::Contain"),
    ("fill", "ObjectFit::Fill"),
    ("cover", "ObjectFit::Cover"),
    ("contain_integer", "ObjectFit::ContainInteger"),
];

/// `cursor:` — the pointer's shape over a box, one spelling per `Cursor` variant.
///
/// A closed set **or** an expression, which is why it is not a [`ValueKind::Keywords`]: that would refuse every expression, and a cursor is as often picked as it is written — `cursor:col_resize` on one strip and `cursor:along(axis)` on the strip that could run either way. A name in this table is the variant it names, and anything else is the Rust expression the author wrote, which is the ladder a colour keyword already takes.
///
/// Read once, when the box is built: a cursor is a value the widget keeps, not a style it re-resolves.
pub const CURSOR_VALUES: &[(&str, &str)] = &[
    ("default", "Cursor::Default"),
    ("pointer", "Cursor::Pointer"),
    ("crosshair", "Cursor::Crosshair"),
    ("grab", "Cursor::Grab"),
    ("grabbing", "Cursor::Grabbing"),
    ("col_resize", "Cursor::ColResize"),
    ("row_resize", "Cursor::RowResize"),
    ("text", "Cursor::Text"),
    ("not_allowed", "Cursor::NotAllowed"),
    ("wait", "Cursor::Wait"),
];

/// The `Cursor` variant `value` names, or `None` when it names none — in which case it is a Rust expression like every other value.
///
/// Matched on the pascal-cased spelling rather than on the table's own, so every way of writing a variant that worked while this key took nothing but a keyword still does: `col_resize`, `col-resize`, `ColResize`.
pub fn cursor_keyword(value: &str) -> Option<&'static str> {
    let wanted = crate::naming::to_pascal_case(value);
    CURSOR_VALUES
        .iter()
        .find(|(_, rust)| rust.strip_prefix("Cursor::") == Some(wanted.as_str()))
        .map(|(_, rust)| *rust)
}

/// `raster:` — how samples meet the pixel grid, for a glyph and for a picture alike.
///
/// One key now, where `text` said `raster:` and `img` said `filter:` for the same decision in two vocabularies. `linear`/`nearest` remain as the picture's own words for the two ends.
pub const RASTER_VALUES: &[(&str, &str)] = &[
    ("smooth", "Raster::Smooth"),
    ("subpixel", "Raster::Smooth"),
    ("linear", "Raster::Smooth"),
    ("pixel", "Raster::Pixel"),
    ("nearest", "Raster::Pixel"),
];

/// `font_weight:` — the OpenType weight axis, named.
///
/// One spelling per step, where there were sixteen for nine values: `semibold`, `semi-bold` and `demibold` were one weight, and a synonym is cost with nothing bought. `heavy` for 900 rather than CSS's `black`, because `font_weight` is writable on a container now and `col font_weight:black color:black` would be two meanings of one word on one line.
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
/// Three-valued where the markup had a bare `italic` flag, because the shaper has modelled oblique all along and nothing could ask for it.
pub const FONT_STYLE_VALUES: &[(&str, &str)] = &[
    ("normal", "FontStyle::Normal"),
    ("italic", "FontStyle::Italic"),
    ("oblique", "FontStyle::Oblique"),
];

/// `text_wrap:` — whether text wraps into its box or keeps one line.
///
/// Named apart from the container's `wrap:`, which is flex-wrap and one character away from the `nowrap` flag this replaces.
pub const TEXT_WRAP_VALUES: &[(&str, &str)] = &[
    ("wrap", "TextWrap::Wrap"),
    ("nowrap", "TextWrap::NoWrap"),
    ("no_wrap", "TextWrap::NoWrap"),
];

/// `text_align:` — where the lines sit inside the text's own box, as against a container's `align:`, which places the box among its siblings.
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

/// What an attribute's value has to be for its key to mean anything, so a value outside it is a build error on the attribute instead of a property quietly dropped or quietly defaulted.
///
/// The counterpart to [`tag_attr_keys`]: that answers which keys a tag has, this answers what those keys take. A key absent from [`value_kind`] carries a value only rustc can judge — a string, a callback, an expression — and is left alone.
#[derive(Clone, Copy)]
pub enum ValueKind {
    /// A closed set of spellings, each paired with the Rust name it generates. Also the completion list.
    Keywords(&'static [(&'static str, &'static str)]),
    /// A closed set of spellings *or* a plain number, for the one axis that is genuinely both: an OpenType weight *is* a number, and the names are the nine steps of it everyone actually writes.
    KeywordsOrNumber(&'static [(&'static str, &'static str)]),
    /// A number: a literal, a `50%`, or any Rust expression that yields one.
    Number,
    /// A yes or a no: the bare key, `true`/`false`, or any Rust expression that yields one. Only the first three are literals — the rest is read, so a style carrying one re-resolves from what it reads.
    Boolean,
    /// One number per edge: a single value, or the CSS 2/3/4-value shorthand.
    Edges,
    /// A colour: a hex literal, `transparent`, a `$signal`, or any Rust expression that yields one.
    Color,
}

/// One attribute of one tag: its spelling, what its value has to be, and what it does.
///
/// The single entry the whole DSL vocabulary is described by. Completion lists [`key`](Self::key), the emitter validates against [`kind`](Self::kind), and hover shows [`doc`](field@Self::doc) — so a key cannot be offered by the editor without the build knowing what it takes, which is what the three parallel tables here used to allow.
#[derive(Clone, Copy)]
pub struct AttrSpec {
    /// The attribute's spelling in the markup.
    pub key: &'static str,
    /// What the value has to be, or `None` when only rustc can judge it — a string, a callback, an arbitrary expression.
    pub kind: Option<ValueKind>,
    /// One line on what the attribute does, or `None` for a key whose name is the whole of it.
    pub doc: Option<&'static str>,
}

impl AttrSpec {
    const fn of(key: &'static str, kind: Option<ValueKind>) -> Self {
        Self {
            key,
            kind,
            doc: None,
        }
    }

    /// A key whose value only rustc can judge.
    const fn free(key: &'static str) -> Self {
        Self::of(key, None)
    }

    const fn num(key: &'static str) -> Self {
        Self::of(key, Some(ValueKind::Number))
    }

    const fn color(key: &'static str) -> Self {
        Self::of(key, Some(ValueKind::Color))
    }

    const fn edges(key: &'static str) -> Self {
        Self::of(key, Some(ValueKind::Edges))
    }

    const fn boolean(key: &'static str) -> Self {
        Self::of(key, Some(ValueKind::Boolean))
    }

    /// A key that is the assertion itself: writing it turns the thing on. See [`FLAG_VALUES`].
    const fn flag(key: &'static str) -> Self {
        Self::of(key, Some(ValueKind::Keywords(FLAG_VALUES)))
    }

    const fn keywords(key: &'static str, table: &'static [(&'static str, &'static str)]) -> Self {
        Self::of(key, Some(ValueKind::Keywords(table)))
    }

    const fn keywords_or_number(
        key: &'static str,
        table: &'static [(&'static str, &'static str)],
    ) -> Self {
        Self::of(key, Some(ValueKind::KeywordsOrNumber(table)))
    }

    const fn doc(self, doc: &'static str) -> Self {
        Self {
            key: self.key,
            kind: self.kind,
            doc: Some(doc),
        }
    }
}

/// The spec for `tag`'s `key`, or `None` when the tag does not take it.
///
/// A component tag has no table of its own — its keys are `Props` fields, which rustc checks — but the layout and paint attributes written on one still mean what they mean on a container, so it is read against `box`.
pub fn attr_spec(tag: &str, key: &str) -> Option<AttrSpec> {
    let lens = if is_builtin_tag(tag) { tag } else { "box" };
    tag_attr_specs(lens)
        .into_iter()
        .find(|spec| spec.key == key)
}

/// The value schema of `tag`'s `key`, or `None` when the key takes a free-form value.
///
/// Tag-aware because one name is still two properties: `stroke` is a colour on a box and a *width* on an `svg`, and the four edges a box collects are one plain number on a `path`. That distinction lives in the per-tag tables rather than here, so a key and its schema cannot be added in different places.
pub fn value_kind(tag: &str, key: &str) -> Option<ValueKind> {
    attr_spec(tag, key).and_then(|spec| spec.kind)
}

/// One line on what `tag`'s `key` does, for hover and completion detail.
pub fn attr_doc(tag: &str, key: &str) -> Option<&'static str> {
    attr_spec(tag, key).and_then(|spec| spec.doc)
}

/// Whether `key` is one of the declarative affine transform attributes (see `container::transform_call`, which gates `.with_transform` emission on exactly this set).
pub fn is_transform_attr(key: &str) -> bool {
    TRANSFORM_ATTRS.iter().any(|spec| spec.key == key)
}

/// Whether `key` is a text property that flows down the tree, so a container may name it for everything beneath it and a leaf takes what it did not name itself.
///
/// The same set on a container and on a `text`, which is the point: `font_size:11` means one thing, and where it is written decides how far it reaches rather than what it says. Absent from it are the properties that clamp *one* paragraph — `lines`, `ellipsis` — which would be nonsense applied to a subtree, and which [`renderer_core::Declared`] has no way to spell for the same reason.
pub fn is_inheritable_text_attr(key: &str) -> bool {
    INHERITABLE_TEXT_ATTRS.iter().any(|spec| spec.key == key)
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

/// The text properties that flow down the tree. See [`is_inheritable_text_attr`].
const INHERITABLE_TEXT_ATTRS: &[AttrSpec] = &[
    AttrSpec::num("font_size"),
    AttrSpec::free("font_family"),
    AttrSpec::keywords_or_number("font_weight", FONT_WEIGHT_VALUES),
    AttrSpec::keywords("font_style", FONT_STYLE_VALUES),
    AttrSpec::color("color"),
    AttrSpec::keywords("text_align", TEXT_ALIGN_VALUES),
    AttrSpec::keywords("text_wrap", TEXT_WRAP_VALUES),
    AttrSpec::num("line_height"),
    AttrSpec::num("letter_spacing"),
    AttrSpec::keywords("raster", RASTER_VALUES),
];

/// The transform attributes, appended to every container's key set. See [`is_transform_attr`].
const TRANSFORM_ATTRS: &[AttrSpec] = &[
    AttrSpec::free("rotate"),
    AttrSpec::free("scale"),
    AttrSpec::free("scale_x"),
    AttrSpec::free("scale_y"),
    AttrSpec::free("translate_x"),
    AttrSpec::free("translate_y"),
];

/// What clamps one paragraph and would be nonsense applied to a subtree, which is why these are a `text`'s alone rather than [`INHERITABLE_TEXT_ATTRS`].
const TEXT_ONLY_ATTRS: &[AttrSpec] = &[
    AttrSpec::free("lines"),
    AttrSpec::flag("ellipsis"),
    AttrSpec::free("transition"),
];

/// Every attribute `tag` accepts, with what each one takes and what it does.
///
/// The authority the rest of the vocabulary is read off: [`tag_attr_keys`] projects the spellings for completion, [`value_kind`] reads the schema, [`attr_doc`] the prose. Mirrors the per-tag attribute handling in `crate::view`; a component tag (not built-in) takes its `Props` fields, so it returns nothing.
pub fn tag_attr_specs(tag: &str) -> Vec<AttrSpec> {
    let with = |extra: &[AttrSpec]| {
        let mut specs = LAYOUT_ATTRS.to_vec();
        specs.extend_from_slice(extra);
        specs
    };
    match tag {
        // A `text` is a leaf in a flex box, so it takes the layout keys that place it among its siblings.
        "text" => {
            let mut specs = with(INHERITABLE_TEXT_ATTRS);
            specs.extend_from_slice(TEXT_ONLY_ATTRS);
            specs
        }
        // Spliced children own their own style, so the placeholder takes no layout or paint keys.
        "children" => vec![AttrSpec::free("name"), AttrSpec::free("in")],
        // box/col/row/grid share one paint set; grid adds its track keys.
        "grid" => {
            let mut specs = with(CONTAINER_PAINT);
            specs.extend_from_slice(TRANSFORM_ATTRS);
            specs.extend_from_slice(INHERITABLE_TEXT_ATTRS);
            specs.extend_from_slice(&[
                AttrSpec::free("cols"),
                AttrSpec::free("span"),
                AttrSpec::free("row_span"),
            ]);
            specs
        }
        "col" | "row" | "box" => {
            let mut specs = with(CONTAINER_PAINT);
            specs.extend_from_slice(TRANSFORM_ATTRS);
            specs.extend_from_slice(INHERITABLE_TEXT_ATTRS);
            // A box is a grid *item* wherever its parent is a grid, so it carries the placement keys.
            specs.extend_from_slice(&[AttrSpec::free("span"), AttrSpec::free("row_span")]);
            specs
        }
        // `radius` rounds the picture itself; a leaf takes no other paint key.
        "img" | "image" => {
            let mut specs = with(&[
                AttrSpec::free("src"),
                AttrSpec::keywords("fit", FIT_VALUES),
                AttrSpec::keywords("raster", RASTER_VALUES),
                AttrSpec::edges("radius"),
            ]);
            specs.extend(
                CONTAINER_PAINT
                    .iter()
                    .filter(|spec| spec.key.starts_with("radius_")),
            );
            specs
        }
        // Names the kept scroll position, so a remounted tree reopens where it was.
        "scroll" => with(&[AttrSpec::free("keep")]),
        "canvas" => with(&[AttrSpec::free("paint")]),
        // `input` amends the text style the tree declared, so it takes the same inheritable keys a `text` does.
        "input" => {
            let mut specs = with(&[
                AttrSpec::free("value"),
                AttrSpec::free("placeholder"),
                AttrSpec::free("on_submit"),
                AttrSpec::free("on_cancel"),
                AttrSpec::flag("secret"),
                // Opens holding the keyboard, and says which id it holds it under.
                AttrSpec::free("autofocus"),
                AttrSpec::free("focus_id"),
            ]);
            specs.extend_from_slice(INHERITABLE_TEXT_ATTRS);
            specs
        }
        // A width here, where every other tag means a colour by the same name.
        "svg" => with(&[
            AttrSpec::free("src"),
            AttrSpec::color("color"),
            AttrSpec::num("stroke"),
            AttrSpec::keywords("fit", FIT_VALUES),
        ]),
        "lazy" => with(&[AttrSpec::free("when")]),
        // A `path`'s stroke is one plain width, not the four `crate::edges` collects for a box, and `fill_rule` is a keyword rather than paint.
        "path" => with(&[
            AttrSpec::free("d"),
            AttrSpec::color("fill"),
            AttrSpec::free("stroke"),
            AttrSpec::free("stroke_width"),
            AttrSpec::free("fill_rule"),
        ]),
        _ if is_builtin_tag(tag) => LAYOUT_ATTRS.to_vec(),
        _ => vec![],
    }
}

/// Completion attribute keys for `tag`: the spellings [`tag_attr_specs`] carries.
pub fn tag_attr_keys(tag: &str) -> Vec<&'static str> {
    tag_attr_specs(tag)
        .into_iter()
        .map(|spec| spec.key)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_and_control_flow_classification() {
        assert!(is_builtin_tag("col") && is_builtin_tag("text"));
        assert!(!is_builtin_tag("feature_card"));
        assert!(!is_builtin_tag("btn") && !is_builtin_tag("heading") && !is_builtin_tag("section"));
        assert!(is_control_flow_keyword("for") && !is_control_flow_keyword("col"));
    }

    #[test]
    fn tag_attr_keys_layer_layout_and_tag_specific() {
        assert!(tag_attr_keys("col").contains(&"gap"));
        assert!(tag_attr_keys("btn").is_empty());
        assert!(tag_attr_keys("img").contains(&"src"));
        let svg = tag_attr_keys("svg");
        assert!(svg.contains(&"src") && svg.contains(&"color") && svg.contains(&"gap"));
        assert!(tag_attr_keys("feature_card").is_empty());
        assert!(tag_attr_keys("box").contains(&"transition"));
        assert!(tag_attr_keys("text").contains(&"transition"));
        assert!(tag_attr_keys("col").contains(&"transition"));
        for tag in ["box", "col", "row", "grid"] {
            for spec in TRANSFORM_ATTRS {
                assert!(
                    tag_attr_keys(tag).contains(&spec.key),
                    "{tag} missing {}",
                    spec.key
                );
            }
        }
    }

    #[test]
    fn color_keywords_match_keyword_color_rgba() {
        assert_eq!(color_keywords(), &["transparent"]);
        assert_eq!(keyword_color_rgba("transparent"), Some([0, 0, 0, 0]));
        assert_eq!(keyword_color_rgba("cerulean"), None);
        assert_eq!(keyword_color_rgba("white"), None);
        assert_eq!(keyword_color_rgba("black"), None);
    }

    #[test]
    fn color_keys_cover_every_attribute_that_paints() {
        for key in ["color", "fill", "stroke", "outline", "shadow_color"] {
            assert!(color_attr_keys().contains(&key), "missing {key}");
        }
        for key in ["gradient", "from", "to", "mid", "mid_pos", "radial_radius"] {
            assert!(!color_attr_keys().contains(&key), "{key} should be gone");
        }
    }
}

/// The roles an application may name on a box, and the variant each one is.
///
/// Written here rather than derived from `semantics-core` because the transpiler emits a *path*, and a path is not something a runtime value can produce. Kept beside the tag tables for the same reason they are here: tooling offers these as completions, and an unknown one is a diagnostic rather than a box that silently means nothing.
///
/// The aliases are the words an author reaches for first. `sidebar` is not an ARIA role — `complementary` is — and pointing one at the other is cheaper than explaining the difference at every use.
pub const ROLE_VALUES: &[(&str, &str)] = &[
    ("group", "Group"),
    ("banner", "Banner"),
    ("header", "Banner"),
    ("navigation", "Navigation"),
    ("nav", "Navigation"),
    ("main", "Main"),
    ("content", "Main"),
    ("complementary", "Complementary"),
    ("aside", "Complementary"),
    ("sidebar", "Complementary"),
    ("contentinfo", "ContentInfo"),
    ("footer", "ContentInfo"),
    ("article", "Article"),
    ("section", "Section"),
    ("form", "Form"),
    ("search", "Search"),
    ("h1", "Heading(1)"),
    ("h2", "Heading(2)"),
    ("h3", "Heading(3)"),
    ("h4", "Heading(4)"),
    ("h5", "Heading(5)"),
    ("h6", "Heading(6)"),
    ("list", "List"),
    ("listitem", "ListItem"),
    ("item", "ListItem"),
    ("dialog", "Dialog"),
    ("button", "Button"),
    ("link", "Link"),
    ("checkbox", "CheckBox"),
    ("radio", "Radio"),
    ("switch", "Switch"),
    ("toggle", "Switch"),
    ("tab", "Tab"),
    ("tabpanel", "TabPanel"),
    ("menuitem", "MenuItem"),
    ("slider", "Slider"),
    ("spinbutton", "SpinButton"),
    ("progressbar", "ProgressBar"),
    ("progress", "ProgressBar"),
    ("label", "Label"),
];

/// The roles an application may name on a box, as [`ROLE_VALUES`].
pub fn role_values() -> &'static [(&'static str, &'static str)] {
    ROLE_VALUES
}

/// The variant `name` spells, or `None` for a word nothing answers to.
pub fn role_variant(name: &str) -> Option<&'static str> {
    role_values()
        .iter()
        .find(|(spelling, _)| *spelling == name)
        .map(|(_, variant)| *variant)
}

#[cfg(test)]
mod role_tests {
    use super::*;

    /// The transpiler emits a path and the runtime parses a name; they are two tables and they have to agree, or a role an author is offered is one the vocabulary does not have.
    #[test]
    fn every_spelling_is_one_the_vocabulary_answers_to() {
        for (name, _) in role_values() {
            assert!(
                semantics_core::Role::parse(name).is_some(),
                "`{name}` is offered but the vocabulary does not know it"
            );
        }
    }
}

#[cfg(test)]
mod vocabulary_tests {
    use super::*;

    /// The emitter and the tables were two lists of the same vocabulary, and they drifted: `aspect`, `aspect_ratio` and `flex_basis` were emitted by `layout_prop_call` for years while completion never offered them and the unknown-attribute check refused them. One table now, and this is what holds it to one.
    #[test]
    fn every_layout_key_offered_is_one_the_emitter_accepts() {
        let probe = |key: &str| match value_kind("box", key) {
            Some(ValueKind::Keywords(table)) | Some(ValueKind::KeywordsOrNumber(table)) => {
                table.first().map(|(name, _)| *name).unwrap_or("")
            }
            Some(ValueKind::Boolean) => "true",
            Some(ValueKind::Color) => "#000000",
            _ => "1",
        };
        for key in layout_attr_keys() {
            assert!(
                !matches!(
                    crate::style::layout_prop_call(key, probe(key)),
                    crate::style::PropCall::Invalid(_)
                ),
                "`{key}` is offered and the emitter refuses it"
            );
        }
        for key in ["aspect", "aspect_ratio", "flex_basis"] {
            assert!(
                matches!(
                    crate::style::layout_prop_call(key, "1"),
                    crate::style::PropCall::Call(_)
                ) && layout_attr_keys().contains(&key),
                "`{key}` drifted out of the table again"
            );
        }
    }

    /// A key is offered by completion and validated by the build off the same entry, so neither can name one the other does not.
    #[test]
    fn every_offered_key_resolves_to_its_own_spec() {
        for (tag, _) in builtin_tags() {
            for key in tag_attr_keys(tag) {
                assert!(
                    attr_spec(tag, key).is_some(),
                    "`{tag}` offers `{key}` and nothing describes it"
                );
            }
        }
    }

    /// One name is still two properties, and the per-tag tables are what keep them apart.
    #[test]
    fn stroke_means_what_the_tag_says_it_means() {
        assert!(matches!(
            value_kind("box", "stroke"),
            Some(ValueKind::Color)
        ));
        assert!(matches!(
            value_kind("svg", "stroke"),
            Some(ValueKind::Number)
        ));
        assert!(value_kind("path", "stroke").is_none());
        assert!(matches!(
            value_kind("box", "stroke_width"),
            Some(ValueKind::Edges)
        ));
        assert!(value_kind("path", "stroke_width").is_none());
    }
}
