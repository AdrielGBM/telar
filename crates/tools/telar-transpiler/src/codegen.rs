//! The codegen engine: turns a parsed [`RsxDocument`] into compilable Rust source, wiring the `[logic]`, `[style]`, `[view]`, and `[preview]` zones together with a per-line source map.

use std::path::Path;

use telar_parser::{RsxDocument, ViewNode};

use crate::error::TranspileError;
use crate::naming::{
    contains_ident, literal_or_comment_end, preview_entries_const_name, replace_whole_word,
    to_pascal_case, to_snake_case,
};
use crate::signal_scan::{scan_effects, scan_locals, scan_signals, unbound_effect};
use crate::source_map::ExprSpan;
use crate::style::generate_style_section;
use crate::view::ViewGen;

/// The call-relevant shape of a component: what its function signature and `Props` struct look like,
/// so a *caller* in another `.rsx` can emit the right arguments (optional props, the slot argument)
/// without seeing the callee's source. Collected across the workspace into a [`ComponentRegistry`].
#[derive(Clone, Debug, Default)]
pub struct ComponentSig {
    /// The component declares a `pub struct Props`, so calls must pass a `Props` argument.
    pub has_props: bool,
    /// `Props` derives `Default`, so a call may omit fields (`..Default::default()` fills the rest).
    pub props_default: bool,
    /// The `Props` field names, so a caller knows when it has omitted some (and must default them).
    pub prop_fields: Vec<String>,
    /// The view uses a `children` slot placeholder, so every call must pass a `Slots` argument — even a
    /// childless one (which passes `Slots::new()`).
    pub has_slot: bool,
    /// Prop fields whose value is a reactive colour: the caller emits them as a `Box<dyn Fn() -> Color>`
    /// closure (re-read each frame) instead of a resolved `Color`, so a theme token or `$signal` colour
    /// re-colours live. Empty for scanned `.rsx` components; set only for the built-in component catalogue.
    pub color_fields: Vec<String>,
    /// Prop fields whose value is a reactive *reading* — a number a widget displays and never writes: the
    /// caller emits them as a `Box<dyn Fn() -> T>` closure, so a value derived from several services can drive
    /// the widget. A prop that insists on a signal is what makes an application reimplement the widget.
    pub reading_fields: Vec<String>,
    /// Prop fields whose value is a reactive string: the caller emits them as a `Box<dyn Fn() -> String>`
    /// closure (re-read each frame) instead of a `&'static str`, so a `t"key"` translation or `$signal`
    /// string re-renders live on a locale/state change. Mirrors [`Self::color_fields`]; scanned from a
    /// `.rsx`'s `Box<dyn Fn() -> String>` fields and listed explicitly for the built-in component catalogue.
    pub text_fields: Vec<String>,
    /// Prop fields the callee declares `Option<...>` (so `Default` yields `None`): the caller wraps a
    /// provided value in `Some(...)` and lets an omitted one fall to `..Default::default()`. Lets a widget
    /// expose a `RwSignal<T>` or a required `Box<dyn Fn(..)>` field — neither of which is `Default` — while
    /// still deriving `Default` for its other props. Scanned from a `.rsx`'s `Option<...>` fields; listed
    /// explicitly for the built-in component catalogue.
    pub optional_fields: Vec<String>,
    /// Prop fields declared as an owned `String`. A quoted value at a call site is a `&str` literal, which
    /// such a field does not take — so the caller had to bind every label to a `[logic]` local just to write
    /// `.to_string()` on it, six lines of preamble for three buttons. Knowing the type, the transpiler can
    /// convert the literal itself.
    pub string_fields: Vec<String>,
    /// Prop fields whose value is a reactive predicate (`Box<dyn Fn() -> bool>`): the caller emits them as a
    /// closure, so `disabled:$cant_undo` re-reads. Mirrors [`Self::text_fields`]; the same shape a `box`'s own
    /// `disabled:` already has, which is why a row in a menu can be written the same way as any other widget.
    pub bool_fields: Vec<String>,
    /// The component takes its children as a `Children` recipe rather than a built `Slots`, so the caller
    /// emits a closure instead of widgets. What makes a *compound* component possible: the callee runs the
    /// recipe inside a context of its own, and its pieces reach it with `use_context` rather than through
    /// props threaded down by hand. See `Children` in ui-core.
    pub defers_children: bool,
}

impl ComponentSig {
    /// Marks `fields` as reactive string props (`Box<dyn Fn() -> String>`), used when declaring the built-in
    /// component catalogue.
    fn with_text(mut self, fields: &[&str]) -> Self {
        self.text_fields = fields.iter().map(|f| f.to_string()).collect();
        self
    }

    /// Marks `fields` as reactive reading props (`Box<dyn Fn() -> T>` over a number the widget displays).
    fn with_readings(mut self, fields: &[&str]) -> Self {
        self.reading_fields = fields.iter().map(|f| f.to_string()).collect();
        self
    }

    /// Marks `fields` as reactive predicate props (`Box<dyn Fn() -> bool>`).
    fn with_bools(mut self, fields: &[&str]) -> Self {
        self.bool_fields = fields.iter().map(|f| f.to_string()).collect();
        self
    }
}

/// Signatures for the built-in component catalogue (`ui-components`, opt-in via the `components` feature). These
/// components are not local `.rsx` files, so `scan_component_sig` never sees them; this seeds the
/// [`ComponentRegistry`] (and backstops call-site lookups) so calls emit the right arity and reactive
/// colour props. Keep in sync with `crates/ui/ui-components`.
pub fn external_component_sigs() -> Vec<(&'static str, ComponentSig)> {
    // `optional` names the subset of `fields` the callee declares `Option<...>`; the caller wraps their
    // values in `Some(...)` and defaults an omitted one to `None`.
    let s = |fields: &[&str], has_slot: bool, color: &[&str], optional: &[&str]| ComponentSig {
        has_props: true,
        props_default: true,
        prop_fields: fields.iter().map(|f| f.to_string()).collect(),
        has_slot,
        color_fields: color.iter().map(|f| f.to_string()).collect(),
        reading_fields: Vec::new(),
        // The catalogue takes its owned-string props through `Box<dyn Fn() -> String>`, so it declares none.
        string_fields: Vec::new(),
        text_fields: Vec::new(),
        optional_fields: optional.iter().map(|f| f.to_string()).collect(),
        bool_fields: Vec::new(),
        defers_children: false,
    };
    // A compound component: its pieces are built inside its own context rather than handed to it already made.
    let compound = |fields: &[&str], color: &[&str], optional: &[&str]| ComponentSig {
        defers_children: true,
        has_slot: true,
        ..s(fields, true, color, optional)
    };
    vec![
        (
            "button",
            s(
                &["label", "fill", "outline", "ghost", "on_press"],
                false,
                &["fill", "outline"],
                &[],
            )
            .with_text(&["label"]),
        ),
        (
            "heading",
            s(&["text"], false, &[], &[]).with_text(&["text"]),
        ),
        (
            "section",
            s(&["title"], true, &[], &[]).with_text(&["title"]),
        ),
        // Form controls (built on box/text/on_press/on_drag/input). A bound `RwSignal` field is `Option`
        // (so `Props` derives `Default`): `None` = uncontrolled, `Some` = caller-bound.
        (
            "checkbox",
            s(
                &["checked", "label", "color", "on_toggle"],
                false,
                &["color"],
                &["checked", "on_toggle"],
            )
            .with_text(&["label"]),
        ),
        (
            "toggle",
            s(
                &["checked", "label", "color", "on_toggle"],
                false,
                &["color"],
                &["checked", "on_toggle"],
            )
            .with_text(&["label"]),
        ),
        (
            "radio",
            s(
                &["selected", "value", "label", "color", "on_select"],
                false,
                &["color"],
                &["selected", "on_select"],
            )
            .with_text(&["label"]),
        ),
        (
            "slider",
            s(
                &[
                    "value",
                    "color",
                    "track_color",
                    "width",
                    "min",
                    "max",
                    "step",
                    "label",
                    "on_change",
                ],
                false,
                &["color", "track_color"],
                &["value", "on_change"],
            )
            .with_text(&["label"]),
        ),
        (
            "text_field",
            s(
                &[
                    "value",
                    "placeholder",
                    "label",
                    "width",
                    "color",
                    "on_submit",
                ],
                false,
                &["color"],
                &["value", "on_submit"],
            )
            .with_text(&["placeholder", "label"]),
        ),
        // Overlay-backed (built on the `overlay` portal + anchor).
        // Compound like `menu`, and for the same reason: its choices are `item` rows written as children. Its
        // trigger names the chosen one through the declaring walk, not through a flat list of strings.
        (
            "select",
            compound(
                &["selected", "color", "on_select", "stretch"],
                &["color"],
                &["selected", "on_select"],
            ),
        ),
        // Compound: its rows are written as `item`/`separator`/`group` children and built inside the
        // `ListContext` it provides, which is why they arrive as a recipe rather than as widgets.
        (
            "menu",
            compound(
                &[
                    "label",
                    "on_select",
                    "color",
                    "stretch",
                    "bordered",
                    "caret",
                    "style",
                ],
                &["color"],
                &["on_select", "style"],
            )
            .with_text(&["label"]),
        ),
        // The pieces a `menu` is written with. Each reads the enclosing list from context, so they take no
        // prop naming their parent and none of them has to be spelled `menu_item`.
        (
            "item",
            s(
                &["label", "disabled", "checked", "hint", "on_press"],
                true,
                &[],
                &["checked", "hint", "on_press"],
            )
            .with_text(&["label", "hint"])
            .with_bools(&["disabled", "checked"]),
        ),
        ("separator", ComponentSig::default()),
        (
            "group",
            s(&["label"], false, &[], &[]).with_text(&["label"]),
        ),
        (
            "modal",
            s(
                &["open", "id", "title", "on_close", "color"],
                true,
                &["color"],
                &["open", "on_close"],
            )
            .with_text(&["title"]),
        ),
        (
            "drawer",
            s(
                &["open", "id", "side", "width", "on_close", "color"],
                true,
                &["color"],
                &["open", "on_close"],
            ),
        ),
        (
            "tooltip",
            s(
                &[
                    "text",
                    "shortcut",
                    "description",
                    "side",
                    "color",
                    "stretch",
                    "style",
                ],
                true,
                &["color"],
                &["style"],
            )
            .with_text(&["text", "shortcut", "description"]),
        ),
        // Presentation & indicators.
        (
            "progress",
            s(
                &[
                    "value",
                    "color",
                    "track_color",
                    "width",
                    "stretch",
                    "height",
                ],
                false,
                &["color", "track_color"],
                &[],
            )
            .with_readings(&["value"]),
        ),
        ("spinner", s(&["color", "size"], false, &["color"], &[])),
        (
            "badge",
            s(&["label", "color"], false, &["color"], &[]).with_text(&["label"]),
        ),
        (
            "chip",
            s(
                &["label", "color", "on_close"],
                false,
                &["color"],
                &["on_close"],
            )
            .with_text(&["label"]),
        ),
        // Navigation & disclosure.
        (
            "tabs",
            s(
                &["items", "selected", "color"],
                false,
                &["color"],
                &["selected"],
            ),
        ),
        (
            "accordion",
            s(&["title", "open", "color"], true, &["color"], &["open"]).with_text(&["title"]),
        ),
        (
            "stepper",
            s(
                &["value", "min", "max", "step", "color", "on_change"],
                false,
                &["color"],
                &["value", "on_change"],
            ),
        ),
    ]
}

/// Maps a component's callable name (both its path-flattened stem and its bare basename) to its
/// [`ComponentSig`]. Built once per build/analyze pass and threaded into every file's transpile.
pub type ComponentRegistry = std::collections::HashMap<String, ComponentSig>;

/// Scans one `.rsx` source for its [`ComponentSig`] (its `Props` shape and whether it takes a slot).
/// A parse failure yields an empty sig, so a temporarily-broken file never poisons the registry.
pub fn scan_component_sig(source: &str) -> ComponentSig {
    let Ok(doc) = telar_parser::parse(source) else {
        return ComponentSig::default();
    };
    let props = scan_props_struct(&doc.logic.source);
    let has_slot = view_uses_slot(&doc.view.nodes);
    ComponentSig {
        has_props: props.has_props,
        props_default: props.props_default,
        prop_fields: props.fields,
        has_slot,
        color_fields: props.color,
        reading_fields: Vec::new(),
        text_fields: props.text,
        optional_fields: props.optional,
        string_fields: props.owned_text,
        bool_fields: Vec::new(),
        // A `Context` struct in `[logic]` is what makes a `.rsx` compound: it is the type the component's
        // children read, so the component takes the recipe for them and runs it inside one. Without somewhere
        // to put children it declares a type nobody is handed, so the slot is half of the condition.
        defers_children: has_slot
            && struct_line_span(&doc.logic.source.lines().collect::<Vec<_>>(), "Context").is_some(),
    }
}

/// What a scanned `struct Props` tells a call site.
#[derive(Default)]
struct ScannedProps {
    has_props: bool,
    /// A preceding `#[derive(…Default…)]`, or an inline field default, makes every prop omittable at a call site.
    props_default: bool,
    fields: Vec<String>,
    /// The subset typed `Option<…>`: the caller `Some(…)`-wraps their values.
    optional: Vec<String>,
    /// The subset typed `Box<dyn Fn() -> String>`: the caller may write a literal or a `$signal` and have it
    /// boxed into a closure. Read off the declared type rather than a hardcoded list, so a `.rsx` component gets
    /// the same live text the built-in catalogue does — without it, a user component can only take
    /// `&'static str` and cannot show a value that changes.
    text: Vec<String>,
    /// The same for `Box<dyn Fn() -> Color>`.
    color: Vec<String>,
    /// The subset typed as a plain owned `String`.
    owned_text: Vec<String>,
}

/// Scans the logic zone for `struct Props`. See [`ScannedProps`] for what each part is used for.
///
/// Located through [`struct_line_span`], the same span `extract_props_struct` lifts into the generated file:
/// what this reads and what that emits cannot disagree about which struct is the props one, nor about which
/// attribute lines belong to it.
fn scan_props_struct(logic: &str) -> ScannedProps {
    let lines: Vec<&str> = logic.lines().collect();
    let Some((start, end)) = struct_line_span(&lines, "Props") else {
        return ScannedProps::default();
    };
    let declared = lines[start..=end]
        .iter()
        .position(|l| declares_struct(l.trim(), "Props"))
        .map_or(start, |rel| start + rel);

    let derives_default = lines[start..declared]
        .iter()
        .any(|l| l.trim_start().starts_with('#') && l.contains("Default"));

    let declaration = lines[declared..=end].join("\n");
    let Some(open) = declaration.find('{') else {
        return ScannedProps {
            has_props: true,
            props_default: derives_default,
            ..Default::default()
        };
    };
    let body_start = open + 1;
    let mut depth = 1i32;
    let mut body_end = body_start;
    for (i, c) in declaration[body_start..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    body_end = body_start + i;
                    break;
                }
            }
            _ => {}
        }
    }
    let mut scanned = ScannedProps {
        has_props: true,
        ..Default::default()
    };
    let mut has_inline_default = false;
    for chunk in split_top_level_commas(&declaration[body_start..body_end]) {
        if let Some(field) = parse_field(&chunk) {
            if field.optional {
                scanned.optional.push(field.name.clone());
            }
            match returned_closure_type(&field.ty) {
                Some("String") => scanned.text.push(field.name.clone()),
                Some("Color") => scanned.color.push(field.name.clone()),
                _ if is_owned_string(&field.ty) => scanned.owned_text.push(field.name.clone()),
                _ => {}
            }
            if field.default.is_some() {
                has_inline_default = true;
            }
            scanned.fields.push(field.name);
        }
    }
    // Inline field defaults synthesize a `Default` impl (see `extract_props_struct`), so the struct is
    // default-constructible even without `#[derive(Default)]` — callers may omit its props like a derived one.
    scanned.props_default = derives_default || has_inline_default;
    scanned
}

/// The type a `Box<dyn Fn() -> T>` prop returns, unqualified (`Color` for `telar::Color`), or `None` when the
/// field is not a nullary closure. This is what marks a prop as taking live text or a live colour, so a call
/// site can pass a literal or a `$signal` and have it boxed.
/// Whether `ty` is a plain owned `String`, `Option<String>` included.
///
/// Deliberately narrow: only the exact type, never a `Vec<String>` or a `HashMap<_, String>`, because the
/// conversion this enables applies to one value and would be wrong for a field that holds many.
fn is_owned_string(ty: &str) -> bool {
    let compact: String = ty.split_whitespace().collect::<Vec<_>>().join("");
    let inner = compact
        .strip_prefix("Option<")
        .and_then(|rest| rest.strip_suffix('>'))
        .unwrap_or(&compact);
    matches!(inner, "String" | "std::string::String")
}

fn returned_closure_type(ty: &str) -> Option<&str> {
    let compact: String = ty.split_whitespace().collect::<Vec<_>>().join(" ");
    if !compact.contains("Fn()") {
        return None;
    }
    let returned = compact
        .rsplit("->")
        .next()?
        .trim()
        .trim_end_matches('>')
        .trim();
    let last = returned.rsplit("::").next()?.trim();
    match last {
        "String" => Some("String"),
        "Color" => Some("Color"),
        _ => None,
    }
}

/// A parsed `Props` field: its name, its type, whether the type is `Option<...>` (→ the caller
/// `Some(...)`-wraps its value), and any inline default expression (the `name: Type = expr` sugar).
struct ParsedField {
    name: String,
    ty: String,
    optional: bool,
    default: Option<String>,
}

/// Finds the byte index of the top-level `=` that separates a field type from an inline default
/// expression (`name: Type = expr`), or `None`. Skips `=` inside angle brackets and the
/// `==`/`=>`/`<=`/`>=`/`!=` operators, and treats `->` as an arrow (not a generic close) so a type
/// like `Box<dyn Fn() -> T>` keeps correct bracket depth.
fn find_default_sep(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    let mut depth = 0i32;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'<' => depth += 1,
            b'>' if i > 0 && b[i - 1] == b'-' => {}
            b'>' => depth = (depth - 1).max(0),
            b'=' if depth == 0 => {
                let prev = if i > 0 { b[i - 1] } else { 0 };
                let next = if i + 1 < b.len() { b[i + 1] } else { 0 };
                if !matches!(prev, b'=' | b'<' | b'>' | b'!') && !matches!(next, b'=' | b'>') {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Splits a struct body into field chunks on top-level commas, so a comma inside a default expression
/// (e.g. `= Color::rgba(1.0, 0.0, 0.0, 1.0)`) or a generic (`Vec<A, B>`) does not split a field.
///
/// Comments and string literals are skipped whole. A field's own doc comment is prose, and prose has commas in
/// it — counting those split the field away from its type and dropped it from the struct without a word, which
/// surfaced much later as a missing field at the first call site.
fn split_top_level_commas(body: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    let b = body.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if let Some(next) = literal_or_comment_end(b, i) {
            i = next;
            continue;
        }
        match c {
            b'(' | b'[' | b'{' | b'<' => depth += 1,
            b')' | b']' | b'}' => depth = (depth - 1).max(0),
            b'>' if i > 0 && b[i - 1] == b'-' => {}
            b'>' => depth = (depth - 1).max(0),
            b',' if depth == 0 => {
                chunks.push(body[start..i].to_string());
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    chunks.push(body[start..].to_string());
    chunks
}

/// Indices of the logic-zone lines that make up a top-level `use` statement, in source order.
///
/// Column 0 is the test, so a `use` inside a nested `fn` or block stays where its author put it. An unterminated
/// statement is left alone rather than guessed at, so a malformed import fails where it was written.
fn hoisted_use_lines(logic: &str) -> Vec<usize> {
    let lines: Vec<&str> = logic.lines().collect();
    let mut hoisted = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].starts_with("use ") {
            let start = i;
            while i < lines.len() && !lines[i].trim_end().ends_with(';') {
                i += 1;
            }
            if i < lines.len() {
                hoisted.extend(start..=i);
            } else {
                i = start;
            }
        }
        i += 1;
    }
    hoisted
}

/// Extracts a `Props` field from a `[pub] name: Type[ = default]` chunk, skipping comment lines.
fn parse_field(chunk: &str) -> Option<ParsedField> {
    let cleaned = chunk
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join(" ");
    let t = cleaned.trim();
    let t = t.strip_prefix("pub ").unwrap_or(t).trim_start();
    let colon = t.find(':')?;
    let name = t[..colon].trim();
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    let rest = t[colon + 1..].trim_start();
    let (ty, default) = match find_default_sep(rest) {
        Some(i) => (rest[..i].trim(), Some(rest[i + 1..].trim().to_string())),
        None => (rest.trim(), None),
    };
    let optional = ty.starts_with("Option<") || ty.starts_with("Option <");
    Some(ParsedField {
        name: name.to_string(),
        ty: ty.to_string(),
        optional,
        default,
    })
}

/// Input to a single transpilation: the parsed document plus the desired component function name (typically derived from the source file stem).
pub(crate) struct TranspileInput<'a> {
    pub document: &'a RsxDocument,
    pub component_name: &'a str,
    /// Concrete theme type path (e.g. `SandboxTheme`). When set, `[style]` color references resolve through `use_theme::<Type>()` instead of `COLOR_*` consts.
    pub theme_type: Option<&'a str>,
    /// Directory of the `.rsx` being transpiled, used to resolve static `svg`/`img` asset paths (`src:"path"`) for build-time baking. `None` when no filesystem anchor is available (e.g. some analyzer/test paths), in which case a static asset yields a `compile_error!`.
    pub base_dir: Option<&'a Path>,
    /// Signatures of every component in the workspace, so a call site emits optional props and the slot argument correctly. `None` (tests, isolated transpiles) falls back to the per-file heuristic: pass a slot arg only when markup children are present, and require every prop field.
    pub registry: Option<&'a ComponentRegistry>,
}

/// The generated Rust source for one `.rsx` file.
pub struct TranspiledSource {
    pub rust_code: String,
    pub preview_names: Vec<String>,
    /// Per generated line (0-based), the 0-based `.rsx` line it originated from, or `None` for boilerplate and transpiler-injected lines. Lets the analyzer map rust-analyzer's diagnostics on the generated code back onto the `.rsx` source.
    pub source_map: Vec<Option<u32>>,
    /// Byte spans of verbatim `[view]` Rust expressions, mapping a `.rsx` source range to the generated Rust. The half of the map that makes a *column* mean something; persisted into the `.rs.map` beside [`Self::source_map`]. See [`ExprSpan`].
    pub expr_spans: Vec<ExprSpan>,
    /// Whether the component takes a `Props` argument, so callers can alias its `Props` type by base name.
    pub has_props: bool,
}

/// Parses `source` and generates Rust for `component_name`, resolving `[style]` colors through `theme_type` when provided so theme switching at runtime takes effect. `base_dir` is the directory of the `.rsx` (its parent), against which static `svg`/`img` asset paths are resolved and baked at build time.
///
/// `registry` is the workspace [`ComponentRegistry`], which lets calls to other components emit optional
/// props and the slot argument correctly (a childless call to a slotted component still passes
/// `Slots::new()`; a call that omits defaultable props adds `..Default::default()`). `None` transpiles the
/// file on its own, which is what a unit test and a single-file check want.
pub fn transpile_source(
    source: &str,
    component_name: &str,
    theme_type: Option<&str>,
    base_dir: Option<&Path>,
    registry: Option<&ComponentRegistry>,
) -> Result<TranspiledSource, TranspileError> {
    let document = telar_parser::parse(source)?;
    transpile(TranspileInput {
        document: &document,
        component_name,
        theme_type,
        base_dir,
        registry,
    })
}

/// Accumulates generated code together with a per-line origin map. Each completed line (terminated by `\n`) records the `.rsx` source line passed when its newline was appended, so callers tag a line by emitting its content and the closing newline with the same `src`.
#[derive(Default)]
struct Code {
    out: String,
    map: Vec<Option<u32>>,
}

impl Code {
    fn push(&mut self, text: &str, src: Option<u32>) {
        for ch in text.chars() {
            self.out.push(ch);
            if ch == '\n' {
                self.map.push(src);
            }
        }
    }
}

fn transpile(input: TranspileInput<'_>) -> Result<TranspiledSource, TranspileError> {
    let doc = input.document;
    let fn_name = to_snake_case(input.component_name);
    if fn_name.is_empty() {
        return Err(TranspileError::Codegen(
            "component name is empty or has no valid identifier characters".into(),
        ));
    }

    let (props_struct, props_default_impl, props_span) =
        extract_props_struct(&doc.logic.source, &fn_name);
    let (context_struct, context_span) = extract_context_struct(&doc.logic.source, &fn_name);
    let has_props = props_struct.is_some();
    let props_type = if has_props {
        to_pascal_case(&fn_name) + "Props"
    } else {
        String::new()
    };
    // Ascending, because the line-number reconstruction below restores them one after another.
    let mut lifted: Vec<(usize, usize)> =
        [props_span, context_span].into_iter().flatten().collect();
    lifted.sort();
    // The struct declarations move to module scope; what is left is the body, emitted byte for byte. The
    // author still writes the bare `Context` to build one, and an alias in the body is what lets them —
    // renaming it here instead would shift every column on the line, and `[logic]` diagnostics land on the
    // columns rustc gave them precisely because it is transpiled 1:1.
    let logic_lifted = without_line_spans(&doc.logic.source, &lifted);
    let logic_source = if lifted.is_empty() {
        &doc.logic.source
    } else {
        &logic_lifted
    };

    let signals = scan_signals(logic_source);

    let style_section = generate_style_section(&doc.style, input.theme_type.as_deref());

    let mut view_gen = ViewGen::with_theme(
        &doc.style.classes,
        &doc.style.constants,
        input.theme_type,
        input.base_dir,
    )
    .with_locals(scan_locals(logic_source))
    .with_signals(signals.iter().map(|s| s.name.clone()).collect())
    .with_effects(scan_effects(logic_source))
    .with_registry(input.registry);
    let view_body = view_gen.generate_root(&doc.view.nodes);
    let uses_theme = view_gen.uses_theme();

    // An effect nobody bound is dropped where it was made: it runs once, seeds correctly, and never fires
    // again. The view can only keep a handle it can name, so refusing is the honest answer — a scanner that
    // silently declines to keep an effect is worse than one that says it cannot.
    let logic = match unbound_effect(logic_source) {
        Some(line) => format!(
            "compile_error!(\"an effect must be bound to a name so the view can keep it alive, or it runs \
             once and stops: {}\");\n{}",
            line.replace('\\', "").replace('"', "'"),
            logic_source.trim_end()
        ),
        None => logic_source.trim_end().to_string(),
    };

    // A `children` placeholder anywhere in the view makes the component take a `Slots` argument.
    let has_slot = view_uses_slot(&doc.view.nodes);
    // A `Context` struct with nowhere to put children is not a compound component — it declares a type nobody
    // is handed, and taking a recipe the body never runs would only produce an unused argument.
    let is_compound = context_struct.is_some() && has_slot;
    let ret = "Result<Box<dyn LayoutItem>, LayoutError>";
    // A compound component takes the *recipe* for its children, so it can build them inside the context it
    // provides. Everything else takes them already built. See `Children` in ui-core for why the order inverts.
    let children_arg = match is_compound {
        true => "children: Children",
        false => "mut __slots: Slots",
    };
    let signature = match (has_props, has_slot) {
        (true, true) => {
            format!("pub fn {fn_name}(props: {props_type}, {children_arg}) -> {ret}")
        }
        (true, false) => format!("pub fn {fn_name}(props: {props_type}) -> {ret}"),
        (false, true) => format!("pub fn {fn_name}({children_arg}) -> {ret}"),
        (false, false) => format!("pub fn {fn_name}() -> {ret}"),
    };

    // 0-based `.rsx` line of `logic_source` line 0, used to map generated lines back to the source.
    let logic_start0 = doc.logic.start_line.saturating_sub(1) as u32;
    // `logic_source` (with the lifted structs removed) line index -> its 0-based `.rsx` line. The spans go back
    // in ascending order, so each comparison is made against a line number the earlier ones have already
    // restored.
    let logic_line_src = |j: usize| -> u32 {
        let mut orig = j;
        for &(start, end) in &lifted {
            if orig >= start {
                orig += end - start + 1;
            }
        }
        logic_start0 + orig as u32
    };

    let mut code = Code::default();
    code.push(
        "// Generated by telar-transpiler — do not edit manually\n",
        None,
    );
    // Silence clippy for the whole generated module: this is machine-emitted code the consumer can't edit, so
    // lints like `clone_on_copy` (a loop var cloned into a closure) or `collapsible_if` are pure noise on
    // `cargo clippy`. Only clippy is suppressed — rustc errors/warnings still surface (and the analyzer maps
    // them back onto the `.rsx` source), so real mistakes in `[logic]`/`[view]` are unaffected.
    code.push("#![allow(clippy::all)]\n", None);
    code.push("#[allow(unused_imports)] use telar::*;\n", None);
    // Each `.rsx` is wired as its own `mod` (so rust-analyzer treats it as a real module and offers completion); `use super::*` re-imports the sibling components the host re-exports, so cross-component calls like `feature_card()` resolve by bare name just as they did under the old `include!`.
    code.push("#[allow(unused_imports)] use super::*;\n", None);

    // The logic zone's own imports, lifted to module scope: `Props` and each `[preview]` are emitted as siblings
    // of the component function, so a `use` left in its body would be out of scope for exactly the declarations
    // most likely to name an imported type. Inside the body the two placements are equivalent.
    let hoisted_uses = hoisted_use_lines(logic_source);
    if !hoisted_uses.is_empty() {
        let lines: Vec<&str> = logic_source.lines().collect();
        for &j in &hoisted_uses {
            let src = Some(logic_line_src(j));
            // Only the statement's own first line takes the attribute; a `use foo::{` spanning several lines
            // would otherwise get one per continuation, in the middle of the braced list.
            if lines[j].starts_with("use ") {
                code.push("#[allow(unused_imports)] ", src);
            }
            code.push(lines[j], src);
            code.push("\n", src);
        }
    }
    code.push("\n", None);

    // At file scope, not inside the fn body: a compound component's children name this type from other files,
    // which is the whole reason it exists.
    if let (Some(struct_code), Some((start, _))) = (&context_struct, context_span) {
        for (k, line) in struct_code.lines().enumerate() {
            let src = Some(logic_start0 + (start + k) as u32);
            code.push(line, src);
            code.push("\n", src);
        }
        code.push("\n", None);
    }

    // Emit Props struct at file scope (not inside the fn body) so the type is reachable from the function signature and from other crate files.
    if let Some(struct_code) = &props_struct {
        let struct_start = props_span.map(|(s, _)| s).unwrap_or(0);
        for (k, line) in struct_code.lines().enumerate() {
            let src = Some(logic_start0 + (struct_start + k) as u32);
            code.push(line, src);
            code.push("\n", src);
        }
        // Synthesized from inline `field: Type = expr` defaults (no source span — it maps to no `.rsx` line).
        if let Some(impl_code) = &props_default_impl {
            for line in impl_code.lines() {
                code.push(line, None);
                code.push("\n", None);
            }
        }
        code.push("\n", None);
    }

    if !style_section.is_empty() {
        code.push(&style_section, None);
        if !style_section.ends_with('\n') {
            code.push("\n", None);
        }
        code.push("\n", None);
    }

    code.push("#[allow(dead_code, unused_variables, unused_mut)]\n", None);
    code.push(&signature, None);
    code.push(" {\n", None);
    // use_theme inside the fn so multiple include!-ed files don't conflict at crate scope.
    if uses_theme {
        code.push("    #[allow(unused_imports)] use telar::use_theme;\n", None);
    }
    // The lifted context type, back under the name the author wrote it as. Injected (no `.rsx` line of its
    // own), which is what keeps the body's own lines identical to the source they came from.
    if context_struct.is_some() {
        code.push(
            &format!(
                "    #[allow(unused_imports)] use {} as Context;\n",
                context_type_name(&fn_name)
            ),
            None,
        );
    }

    if !logic.is_empty() {
        // Set by cargo-telar for hot-reload builds (the transpiler runs inside the app's proc macro); keyed signals let the dev host snapshot/restore state across dylib swaps.
        let hot_build = std::env::var("TELAR_HOT_RELOAD_BUILD").is_ok();
        // Argument-context depth carried across lines: a `move` closure sitting at depth 0 starts a statement
        // (a `let clone;` can precede it), but at depth > 0 it is an argument inside an unclosed call/array,
        // where a preceding statement would be invalid Rust — there the clone must wrap the closure instead.
        let mut arg_depth = 0i32;
        for (j, line) in logic.lines().enumerate() {
            let src = Some(logic_line_src(j));
            if hoisted_uses.contains(&j) {
                continue;
            }
            if line.is_empty() {
                code.push("\n", src);
                continue;
            }
            // If this line has a `move` closure that captures a previously declared signal, clone the signal
            // under a mangled name for the closure and rewrite the closure to capture that clone instead — so
            // the original binding stays usable by the view/later logic.
            let mut emitted_line = line.to_string();
            if hot_build
                && let Some(rewritten) =
                    crate::signal_scan::hot_rewrite_signal_decl(&emitted_line, &fn_name)
            {
                emitted_line = rewritten;
            }
            let line_start_depth = arg_depth;
            arg_depth += arg_depth_delta(line);
            if line.contains("move") {
                // `scan_signals` already recorded each signal's declaring line index, so "declared above this line" is a lookup, not a re-parse (and it no longer misses type-annotated `let name: T = signal(...)` bindings).
                let captured: Vec<&str> = signals
                    .iter()
                    .filter(|s| s.line_index < j && contains_ident(line, &s.name))
                    .map(|s| s.name.as_str())
                    .collect();
                if line_start_depth > 0 {
                    // Inside call args: wrap just the closure in a clone block so it stays a valid expression.
                    emitted_line = wrap_closure_clones(&emitted_line, &captured);
                } else {
                    for name in &captured {
                        let mv_name = format!("{name}_rsx_mv");
                        // Injected clone: no `.rsx` counterpart.
                        code.push(&format!("    let {mv_name} = {name}.clone();\n"), None);
                        emitted_line = replace_whole_word(&emitted_line, name, &mv_name);
                    }
                }
            }
            code.push(&format!("    {emitted_line}\n"), src);
        }
        code.push("\n", None);
    }

    // Run the children's recipe now that `[logic]` has built the context to run it in, and hand the result to
    // the `children` placeholders as the same `__slots` an eager component receives — so a component with two
    // slots drains one build rather than making one per placeholder.
    if is_compound {
        let build = match slot_context_expr(&doc.view.nodes) {
            Some(ctx) => format!("children.build_with({ctx})?"),
            None => "children.build()?".to_string(),
        };
        code.push(&format!("    let mut __slots = {build};\n"), None);
    }

    // The view body carries source markers from generation; resolve them into per-line origins (for diagnostics) plus the byte spans of verbatim expressions. `view_prefix_len` is the body's start offset in the final file, so each span's relative offset rebases onto the generated file.
    let view_prefix_len = code.out.len();
    let resolved = crate::view::resolve_source_map(&view_body);
    for (line, src) in &resolved.lines {
        code.push(line, *src);
        code.push("\n", *src);
    }
    let mut expr_spans: Vec<ExprSpan> = resolved
        .expr_spans
        .iter()
        .map(|&(rel, rsx_start, len)| ExprSpan {
            rsx_start,
            len,
            gen_start: (view_prefix_len + rel) as u32,
        })
        .collect();
    if !code.out.ends_with('\n') {
        code.push("\n", None);
    }
    code.push("}\n", None);

    if !doc.previews.is_empty() {
        // Each preview is its own build fn — so a prop-taking component can be previewed via its markup body — plus a PreviewEntry the bundler collects. The body reuses the view codegen with no signals in scope (a preview has no `[logic]`).
        for (i, preview) in doc.previews.iter().enumerate() {
            let pfn = format!("{fn_name}_preview_{i}");
            let mut pgen = ViewGen::with_theme(
                &doc.style.classes,
                &doc.style.constants,
                input.theme_type,
                input.base_dir,
            )
            .with_registry(input.registry);
            let pbody = pgen.generate_root(&preview.body);
            code.push("\n", None);
            code.push("#[allow(dead_code, unused_variables, unused_mut)]\n", None);
            code.push(
                &format!("pub fn {pfn}() -> Result<Box<dyn LayoutItem>, LayoutError> {{\n"),
                None,
            );
            if pgen.uses_theme() {
                code.push("    #[allow(unused_imports)] use telar::use_theme;\n", None);
            }
            // `[preview "Name" fixture:path::to::fn]` seeds whatever ambient state this component reads. A path rather than a name declared in `[logic]`, because the logic zone is emitted *inside* the component function while a preview is a sibling function that cannot see into it; the generated module's own `use super::*` resolves a bare name at the crate root. Per-preview only — the process-wide half (theme, locale, config) belongs in the `setup` closure `telar::dev_entry` runs once.
            if let Some(fixture) = preview_fixture(preview) {
                code.push(&format!("    {fixture}();\n"), None);
            }
            let prefix = code.out.len();
            let resolved = crate::view::resolve_source_map(&pbody);
            for (line, src) in &resolved.lines {
                code.push(line, *src);
                code.push("\n", *src);
            }
            for &(rel, rsx_start, len) in &resolved.expr_spans {
                expr_spans.push(ExprSpan {
                    rsx_start,
                    len,
                    gen_start: (prefix + rel) as u32,
                });
            }
            if !code.out.ends_with('\n') {
                code.push("\n", None);
            }
            code.push("}\n", None);
        }

        code.push("\n", None);
        let const_name = preview_entries_const_name(&fn_name);
        code.push(
            &format!("pub const {const_name}: &[::telar::PreviewEntry] = &[\n"),
            None,
        );
        for (i, preview) in doc.previews.iter().enumerate() {
            let pfn = format!("{fn_name}_preview_{i}");
            code.push(
                &format!(
                    "    ::telar::PreviewEntry {{ component_name: \"{fn_name}\", preview_name: \"{}\", build: {pfn}, surface: {} }},\n",
                    preview.name.replace('"', "\\\""),
                    preview_surface(preview)
                ),
                None,
            );
        }
        code.push("];\n", None);
    }

    Ok(TranspiledSource {
        rust_code: code.out,
        preview_names: doc.previews.iter().map(|p| p.name.clone()).collect(),
        source_map: code.map,
        expr_spans,
        has_props,
    })
}

/// Net change in argument-context depth for `line`: `(`/`[` open it, `)`/`]` close it. Block braces `{}` are
/// statement contexts (a `let` is valid inside them), so they don't count. Literals and comments are skipped
/// so brackets inside them don't miscount. Used by the `[logic]` signal-clone pass to tell a statement-start
/// line from a continuation line inside an open call/array.
fn arg_depth_delta(line: &str) -> i32 {
    let bytes = line.as_bytes();
    let mut depth = 0i32;
    let mut i = 0;
    while i < bytes.len() {
        if let Some(end) = literal_or_comment_end(bytes, i) {
            i = end;
            continue;
        }
        match bytes[i] {
            b'(' | b'[' => depth += 1,
            b')' | b']' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    depth
}

/// Byte index of the first whole-word `move` keyword in `line`, or `None`.
fn find_move_keyword(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let is_word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut from = 0;
    while let Some(rel) = line[from..].find("move") {
        let pos = from + rel;
        let before_ok = pos == 0 || !is_word(bytes[pos - 1]);
        let after_ok = bytes.get(pos + 4).is_none_or(|&b| !is_word(b));
        if before_ok && after_ok {
            return Some(pos);
        }
        from = pos + 4;
    }
    None
}

/// Byte index just past the closure that begins at `start`: scans its body tracking bracket depth and stops
/// at the first depth-0 `,` or the first closing bracket that would pop past the closure's own nesting (i.e.
/// one that closes the *enclosing* call), or end of line. Lets a continuation-line closure argument be
/// wrapped without swallowing the surrounding call's `)`/`,`. Literals and comments are skipped, so a `}` or
/// a `,` written inside one does not end the closure early.
fn closure_end(line: &str, start: usize) -> usize {
    let bytes = line.as_bytes();
    let mut depth = 0i32;
    let mut i = start;
    while i < bytes.len() {
        if let Some(end) = literal_or_comment_end(bytes, i) {
            i = end;
            continue;
        }
        match bytes[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' if depth == 0 => break,
            b')' | b']' | b'}' => depth -= 1,
            b',' if depth == 0 => break,
            _ => {}
        }
        i += 1;
    }
    i
}

/// Wraps the `move` closure on `line` in a clone block — `{ let x_rsx_mv = x.clone(); move |..| ..x_rsx_mv.. }`
/// — for every `captured` signal it references, renaming those signals inside the closure. Used when the
/// closure is a call argument (depth > 0), where a preceding `let` statement would be invalid Rust. Returns
/// `line` unchanged when there is no `move` keyword or nothing to capture.
fn wrap_closure_clones(line: &str, captured: &[&str]) -> String {
    if captured.is_empty() {
        return line.to_string();
    }
    let Some(mpos) = find_move_keyword(line) else {
        return line.to_string();
    };
    let end = closure_end(line, mpos);
    let mut inner = line[mpos..end].to_string();
    let mut clones = String::new();
    for name in captured {
        let mv = format!("{name}_rsx_mv");
        inner = replace_whole_word(&inner, name, &mv);
        clones.push_str(&format!("let {mv} = {name}.clone(); "));
    }
    format!("{}{{ {clones}{inner} }}{}", &line[..mpos], &line[end..])
}

/// Whether any node in the view tree is a `children` slot placeholder, so the component function must
/// take a `Slots` argument. Recurses through element children and `if`/`for` branches.
/// The `fixture:` header option of a `[preview]`, if it names one. Quoted or bare, both spellings reach the same
/// path — `fixture:"mock_env"` and `fixture:mock_env` are the same request.
fn preview_fixture(preview: &telar_parser::Preview) -> Option<String> {
    let value = preview
        .options
        .iter()
        .find(|option| option.key == "fixture")?
        .value
        .trim()
        .trim_matches('"');
    (!value.is_empty()).then(|| value.to_string())
}

/// The `surface:WxH` header option of a `[preview]`, as the `Option<PreviewSurface>` its entry carries.
///
/// `[preview "Float" surface:360x240]` renders the component the way the runner mounts a surface — inside a box
/// of that size, under the root that plays the enter transition — instead of as one more widget in the page's
/// column. The bare `animate` flag beside it asks for that transition to run, which is how a preview shows what
/// opening the surface looks like rather than only what it settles to.
fn preview_surface(preview: &telar_parser::Preview) -> String {
    let Some(size) = preview
        .options
        .iter()
        .find(|option| option.key == "surface")
        .map(|option| option.value.trim().trim_matches('"'))
    else {
        return "None".to_string();
    };
    let Some((width, height)) = size
        .split_once(['x', 'X'])
        .and_then(|(w, h)| Some((w.trim().parse::<f32>().ok()?, h.trim().parse::<f32>().ok()?)))
    else {
        // A size that does not parse is a preview the author meant to be a surface, so falling back to a tree
        // would answer a question they did not ask. The generated code names it instead.
        return format!(
            "compile_error!(\"[preview] surface: expects WIDTHxHEIGHT, e.g. surface:360x240 (got {})\")",
            size.replace('"', "'")
        );
    };
    let animate = preview
        .options
        .iter()
        .any(|option| option.key == "animate" && option.value.is_empty());
    format!(
        "Some(::telar::PreviewSurface {{ width: {width:?}, height: {height:?}, animate: {animate} }})"
    )
}

fn view_uses_slot(nodes: &[ViewNode]) -> bool {
    nodes.iter().any(node_uses_slot)
}

fn node_uses_slot(node: &ViewNode) -> bool {
    match node {
        ViewNode::Element(el) => el.tag == "children" || view_uses_slot(&el.children),
        ViewNode::IfBlock(b) => {
            view_uses_slot(&b.then_branch) || b.else_branch.as_deref().is_some_and(view_uses_slot)
        }
        ViewNode::ForBlock(b) => view_uses_slot(&b.body),
        ViewNode::MatchBlock(b) => b.arms.iter().any(|arm| view_uses_slot(&arm.body)),
        ViewNode::LetStmt(_) | ViewNode::Comment(_) => false,
    }
}

/// The `in:` value of a `children` placeholder — the context a compound component builds its children inside.
/// `None` when no placeholder names one, which means the children are built with nothing to read.
///
/// Read off the view rather than off the placeholder that emits it, because the build happens once for the
/// whole component: two `children` placeholders drain one build, exactly as two eager slot placeholders drain
/// one `Slots`.
fn slot_context_expr(nodes: &[ViewNode]) -> Option<String> {
    nodes.iter().find_map(|node| match node {
        ViewNode::Element(el) if el.tag == "children" => el
            .attributes
            .iter()
            .find(|a| a.key == "in")
            .map(|a| a.value.trim().to_string())
            .or_else(|| slot_context_expr(&el.children)),
        ViewNode::Element(el) => slot_context_expr(&el.children),
        ViewNode::IfBlock(b) => slot_context_expr(&b.then_branch)
            .or_else(|| b.else_branch.as_deref().and_then(slot_context_expr)),
        ViewNode::ForBlock(b) => slot_context_expr(&b.body),
        ViewNode::MatchBlock(b) => b.arms.iter().find_map(|arm| slot_context_expr(&arm.body)),
        ViewNode::LetStmt(_) | ViewNode::Comment(_) => None,
    })
}

/// Extracts `pub struct Props { … }` (plus any preceding `#[…]` attribute lines) from the logic zone,
/// renames it to `{PascalFnName}Props`, and returns `(struct_code, default_impl, span)`.
/// `default_impl` is `Some` only when the struct uses inline `field: Type = expr` defaults (a synthesized
/// `Default` impl); it is emitted after the struct with no source mapping. `span` is the struct's
/// `[start, end]` (inclusive) line span within `logic`, so the caller can map the struct back to source.
type ExtractedProps = (Option<String>, Option<String>, Option<(usize, usize)>);

/// Whether `line` declares `struct <name>`. The boundary test matters: without it `struct Context` would also
/// claim a `struct ContextMenu`, and `struct Props` a `struct PropsBag`.
fn declares_struct(line: &str, name: &str) -> bool {
    let needle = format!("struct {name}");
    line.find(&needle).is_some_and(|at| {
        !line[at + needle.len()..].starts_with(|c: char| c.is_alphanumeric() || c == '_')
    })
}

/// The inclusive line span of `struct <name> { … }` in `lines`, taking in any `#[…]` attribute and comment
/// lines directly above it: a doc comment left behind would land on whatever statement follows, describing the
/// wrong thing in the one place a reader of the generated crate would look. `None` when the struct is absent
/// or its braces never close.
fn struct_line_span(lines: &[&str], name: &str) -> Option<(usize, usize)> {
    let declared = lines.iter().position(|l| declares_struct(l.trim(), name))?;

    let mut start = declared;
    while start > 0 && {
        let above = lines[start - 1].trim();
        above.starts_with('#') || above.starts_with("//")
    } {
        start -= 1;
    }

    let mut depth = 0i32;
    for (i, line) in lines[declared..].iter().enumerate() {
        for c in line.chars() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some((start, declared + i));
                    }
                }
                _ => {}
            }
        }
    }
    None
}

/// Lifts the `Context` struct of a compound component out of `[logic]`, renamed to `{PascalFnName}Context`.
///
/// Same lifting `Props` gets, and for the same reason plus one: a type declared inside the function body is
/// not nameable from outside it, and a compound component's context exists precisely to be named by the
/// children — which live in other files. The rename is what keeps two compound components in one crate from
/// both re-exporting a type called `Context`.
fn extract_context_struct(logic: &str, fn_name: &str) -> (Option<String>, Option<(usize, usize)>) {
    let lines: Vec<&str> = logic.lines().collect();
    let Some((start, end)) = struct_line_span(&lines, "Context") else {
        return (None, None);
    };
    let renamed = lines[start..=end].join("\n").replace(
        "struct Context",
        &format!("struct {}", context_type_name(fn_name)),
    );
    (Some(renamed), Some((start, end)))
}

fn context_type_name(fn_name: &str) -> String {
    to_pascal_case(fn_name) + "Context"
}

/// Removes the given inclusive line spans from `logic`, keeping every other line untouched.
fn without_line_spans(logic: &str, spans: &[(usize, usize)]) -> String {
    let lines: Vec<&str> = logic.lines().collect();
    lines
        .iter()
        .enumerate()
        .filter(|(i, _)| !spans.iter().any(|&(s, e)| *i >= s && *i <= e))
        .map(|(_, l)| *l)
        .collect::<Vec<_>>()
        .join("\n")
}

fn extract_props_struct(logic: &str, fn_name: &str) -> ExtractedProps {
    let lines: Vec<&str> = logic.lines().collect();

    let Some((start, end)) = struct_line_span(&lines, "Props") else {
        return (None, None, None);
    };

    let struct_code = lines[start..=end].join("\n");
    let props_type = to_pascal_case(fn_name) + "Props";
    // Only rename the struct declaration, not the `derive(Props)` attribute.
    let renamed = struct_code.replace("struct Props", &format!("struct {props_type}"));
    let span = Some((start, end));

    // Inline `name: Type = expr` defaults: strip them from the emitted struct (Rust fields can't carry
    // a default) and synthesize a `Default` impl instead. Absent any inline default, the struct is
    // emitted verbatim (renamed) — byte-identical to the pre-sugar behaviour.
    let (open_rel, close_rel) = match (renamed.find('{'), renamed.rfind('}')) {
        (Some(o), Some(c)) if o < c => (o, c),
        _ => return (Some(renamed), None, span),
    };
    let body = &renamed[open_rel + 1..close_rel];
    let parsed: Vec<ParsedField> = split_top_level_commas(body)
        .iter()
        .filter_map(|c| parse_field(c))
        .collect();
    if !parsed.iter().any(|f| f.default.is_some()) {
        return (Some(renamed), None, span);
    }

    // Rebuild the struct with defaults stripped, dropping `Default` from any `#[derive(...)]` (it would
    // collide with the synthesized impl). Field-level comments are dropped in the generated output only.
    let header = &renamed[..open_rel];
    let mut struct_out = String::new();
    for line in header.lines() {
        if let Some(kept) = strip_default_from_derive(line) {
            struct_out.push_str(&kept);
            struct_out.push('\n');
        }
    }
    let brace_line = struct_out.trim_end().to_string();
    struct_out.clear();
    struct_out.push_str(&brace_line);
    struct_out.push_str(" {\n");
    for f in &parsed {
        struct_out.push_str(&format!("    pub {}: {},\n", f.name, f.ty));
    }
    struct_out.push('}');

    let mut impl_body = String::new();
    for f in &parsed {
        let val = f
            .default
            .clone()
            .unwrap_or_else(|| "Default::default()".to_string());
        impl_body.push_str(&format!("            {}: {},\n", f.name, val));
    }
    let impl_code = format!(
        "impl Default for {props_type} {{\n    fn default() -> Self {{\n        Self {{\n{impl_body}        }}\n    }}\n}}"
    );

    (Some(struct_out), Some(impl_code), span)
}

/// Removes `Default` from a `#[derive(...)]` attribute line (returning `None` if the derive becomes
/// empty, so the caller drops the line); any non-derive line passes through unchanged.
fn strip_default_from_derive(line: &str) -> Option<String> {
    let t = line.trim();
    if !t.starts_with("#[derive(") {
        return Some(line.to_string());
    }
    let inner_start = t.find('(')? + 1;
    let inner_end = t.rfind(')')?;
    let items: Vec<&str> = t[inner_start..inner_end]
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && *s != "Default")
        .collect();
    if items.is_empty() {
        return None;
    }
    Some(format!("#[derive({})]", items.join(", ")))
}
