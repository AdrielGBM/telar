//! Generates the `[style]` section: color/number constants and per-class `LayoutStyle` constructor functions.

use std::fmt::Write;

use telar_parser::{StyleClass, StyleConstant, StyleSection, StyleValue};

use crate::naming::{constant_name, is_ident, is_path_expr, style_function_name, to_snake_case};
use crate::registry;

/// The names a bare value may resolve against where it is written: the theme type in effect, this file's
/// `[style]` constants, and the `[logic]` bindings in scope.
///
/// Carried rather than passed piecemeal because the answer differs by position and the difference matters: a
/// `[style]` class function is emitted outside the component, so it can see the constants and no bindings,
/// while an attribute in `[view]` can see both. Given that, the same resolution is correct in both places
/// instead of approximately correct in one.
#[derive(Clone, Copy, Default)]
pub struct Scope<'a> {
    pub theme: Option<&'a str>,
    pub constants: &'a [StyleConstant],
    pub locals: &'a [String],
}

impl<'a> Scope<'a> {
    /// The scope of a `[style]` class function: this file's constants and the theme, with no bindings.
    fn style_section(constants: &'a [StyleConstant], theme: Option<&'a str>) -> Self {
        Self {
            theme,
            constants,
            locals: &[],
        }
    }

    fn constant(&self, name: &str) -> Option<&StyleValue> {
        self.constants
            .iter()
            .find(|c| c.name == name)
            .map(|c| &c.value)
    }

    /// A number for a key [`crate::registry::value_kind`] describes, where a value the key cannot mean has
    /// already been reported on the attribute itself. What stands in its place only has to be *something*:
    /// the build stops before anything reads it.
    pub fn number_or(self, value: &str, fallback: &str) -> String {
        format_number(value, self).unwrap_or_else(|_| fallback.to_string())
    }

    /// A number in a position with no attribute of its own to carry a diagnostic — a nested
    /// `hover_style(…)` property — where the error has to travel inside the expression or not at all.
    pub fn number_or_error(self, value: &str) -> String {
        format_number(value, self).unwrap_or_else(|message| {
            format!("compile_error!({})", crate::view::rust_str(&message))
        })
    }
}

/// What a `key:value` attribute contributes to a `LayoutStyle`.
pub enum PropCall {
    /// The builder call to chain on.
    Call(String),
    /// Not a layout property — a paint or behaviour key its own emitter handles.
    Other,
    /// A value this key cannot mean, and why. Reported on the attribute rather than dropped, which is the
    /// whole difference between a misspelled key and a misspelled value having a diagnostic.
    Invalid(String),
}

/// A `theme.field` reference resolved to a reactive theme read, or `None` when `value` is not one (or no
/// theme is configured, in which case the reference is left alone for rustc to reject by name).
///
/// The dotted form is what lets a **non-color** token reach the theme. A bare ident already means "a `[style]`
/// constant" everywhere except color attributes, so overloading it would silently change what existing
/// `pad:card_gap` means; `theme.card_gap` is unambiguous and works in any attribute position.
pub fn theme_field_expr(value: &str, theme: Option<&str>) -> Option<String> {
    let rest = value.trim().strip_prefix("theme.")?;
    let theme = theme?;
    let head: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if head.is_empty() || !is_ident(&head) {
        return None;
    }
    // Whatever follows that first name is the author's own Rust — a call's arguments, a chain onto the value it
    // returned — and is carried through untouched. A theme is a type the *application* declares, so its
    // vocabulary is not something the DSL can enumerate: `font(FontRole::Body)` and `accent.darken(0.1)` are as
    // much "read the theme" as a bare field is, and without them each one has to be hoisted into `[logic]` and
    // referred to by a name the view invents for it.
    let tail = &rest[head.len()..];
    if !tail.is_empty() && !tail.starts_with('(') && !tail.starts_with('.') {
        return None;
    }
    if !balanced(tail) {
        return None;
    }
    Some(format!(
        "use_theme::<{theme}>().{}{tail}",
        to_snake_case(&head)
    ))
}

/// Whether every bracket in `s` is closed, so a half-written call falls through to the arm that would have
/// handled it instead of being emitted as a theme read that cannot compile.
fn balanced(s: &str) -> bool {
    let mut depth = 0i32;
    for c in s.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return false;
        }
    }
    depth == 0
}

/// Renders all constants and style functions for the document's style section. When a theme is active, `[style]` color constants are omitted: color references resolve through `use_theme` instead (see `color_expr`), so they react to theme switches. Number/raw constants are always emitted.
pub fn generate_style_section(section: &StyleSection, theme: Option<&str>) -> String {
    let theme_active = theme.is_some();
    let mut out = String::new();

    let mut emitted_const = false;
    for c in &section.constants {
        // With a theme active the color const would be dead code (every use goes through use_theme), so skip it.
        if theme_active && matches!(c.value, StyleValue::Hex(_)) {
            continue;
        }
        out.push_str(&generate_constant(c));
        out.push('\n');
        emitted_const = true;
    }

    if emitted_const && !section.classes.is_empty() {
        out.push('\n');
    }

    let scope = Scope::style_section(&section.constants, theme);
    for (i, class) in section.classes.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&generate_class_function(class, scope));
        out.push('\n');
    }

    out
}

/// A `[style]` constant, in the Rust type its value already is. All three kinds are emitted: a name declared
/// and not yet referenced is the author's business, the way a class function nobody calls is.
fn generate_constant(c: &StyleConstant) -> String {
    let (name, ty, value) = match &c.value {
        StyleValue::Hex(hex) => (
            constant_name("COLOR_", &c.name),
            "Color",
            hex_to_color_expr(hex),
        ),
        StyleValue::Number(n) => (constant_name("SIZE_", &c.name), "f32", format_f32(*n)),
        StyleValue::Raw(raw) => (constant_name("RAW_", &c.name), "&str", format!("{raw:?}")),
    };
    format!("#[allow(dead_code)]\nconst {name}: {ty} = {value};")
}

/// What a class may carry beyond the layout keys: what a box paints with, and what flows down to the text
/// below it. Anything else is a typo, and used to be dropped on the floor.
fn is_style_key(key: &str) -> bool {
    crate::view::is_paint_key(key)
        || crate::registry::INHERITABLE_TEXT_KEYS.contains(&key)
        || matches!(key, "lines" | "ellipsis")
}

fn generate_class_function(class: &StyleClass, scope: Scope<'_>) -> String {
    let mut out = String::new();
    // A class property has no element and so no attribute line; naming the class is what locates it.
    //
    // The *key* is checked here as well as the value, which an element's attributes get from
    // `unknown_attr_errors` and a class never did: a class property nobody recognises is dropped on the
    // floor, so a renamed key goes on compiling and quietly stops laying the class out.
    for prop in &class.props {
        let message = match layout_prop_call(&prop.key, &prop.value, scope) {
            PropCall::Invalid(message) => Some(message),
            PropCall::Other if !is_style_key(&prop.key) => {
                Some(format!("`{}` is not a style property", prop.key))
            }
            _ => None,
        };
        if let Some(message) = message {
            let _ = writeln!(
                out,
                "compile_error!({});",
                crate::view::rust_str(&format!("in `@{}`: {message}", class.name))
            );
        }
    }
    // A paint-only class (or one only ever used as a non-first, composed class) never has its layout fn
    // called — its paint reaches the RectStyle and its layout props are inlined at the call site — so the
    // generated fn can be dead. It's machine-generated, so silence the lint rather than special-casing it.
    out.push_str("#[allow(dead_code)]\n");
    let _ = writeln!(
        out,
        "fn {}() -> LayoutStyle {{",
        style_function_name(&class.name)
    );
    out.push_str("    LayoutStyle::new()");
    for prop in &class.props {
        if let PropCall::Call(call) = layout_prop_call(&prop.key, &prop.value, scope) {
            let _ = write!(out, "\n        {call}");
        }
    }
    out.push_str("\n}");
    out
}

/// Maps a style property to the `LayoutStyle` builder call it contributes — or reports the value as one this
/// key cannot mean, which is what stops a misspelling from being a property that silently does nothing.
pub fn layout_prop_call(key: &str, value: &str, scope: Scope<'_>) -> PropCall {
    match layout_call(key, value.trim(), scope) {
        Ok(Some(call)) => PropCall::Call(call),
        Ok(None) => PropCall::Other,
        Err(message) => PropCall::Invalid(message),
    }
}

fn layout_call(key: &str, value: &str, scope: Scope<'_>) -> Result<Option<String>, String> {
    let call = match key {
        "width" => format!(".width({})", dimension(value, scope)?),
        "height" => format!(".height({})", dimension(value, scope)?),
        "min_width" => format!(".min_width({})", dimension(value, scope)?),
        "min_height" => format!(".min_height({})", dimension(value, scope)?),
        "max_width" => format!(".max_width({})", dimension(value, scope)?),
        "max_height" => format!(".max_height({})", dimension(value, scope)?),
        "basis" | "flex_basis" => format!(".flex_basis({})", dimension(value, scope)?),
        "aspect" | "aspect_ratio" => format!(".aspect_ratio({})", format_number(value, scope)?),
        "wrap" => format!(".{}()", keyword(key, value, registry::WRAP_VALUES)?),
        // Per-child cross-axis alignment override, e.g. `self:stretch` over a parent `align:center`, or
        // `self:center` to keep a fixed-size child centered instead of stretched.
        "self" => format!(".{}()", keyword(key, value, registry::SELF_VALUES)?),
        "padding" | "pad" => format!(".padding_all({})", format_number(value, scope)?),
        "padding_x" | "pad_x" => format!(".padding_horizontal({})", format_number(value, scope)?),
        "padding_y" | "pad_y" => format!(".padding_vertical({})", format_number(value, scope)?),
        // Logical edges: resolved to left/right against the active writing direction at layout time, so one build serves LTR and RTL.
        "padding_start" | "pad_start" => {
            format!(".padding_start({})", format_number(value, scope)?)
        }
        "padding_end" | "pad_end" => format!(".padding_end({})", format_number(value, scope)?),
        "margin_start" => format!(".margin_inline_start({})", format_number(value, scope)?),
        "margin_end" => format!(".margin_inline_end({})", format_number(value, scope)?),
        "inset_start" => format!(".inset_start({})", format_number(value, scope)?),
        "inset_end" => format!(".inset_end({})", format_number(value, scope)?),
        "inset_top" => format!(".inset_top({})", format_number(value, scope)?),
        "inset_bottom" => format!(".inset_bottom({})", format_number(value, scope)?),
        // Out of flow, pinned only by the insets the author names. `absolute_fill` is the all-four-at-zero
        // shorthand `overlay` uses; a floating panel wants three edges and its own size on the fourth.
        "absolute" => format!(".{}()", keyword(key, value, registry::ABSOLUTE_VALUES)?),
        "gap" => format!(".gap({})", format_number(value, scope)?),
        "gap_x" => format!(".gap_x({})", format_number(value, scope)?),
        "gap_y" => format!(".gap_y({})", format_number(value, scope)?),
        "grow" => format!(".flex_grow({})", format_number(value, scope)?),
        "shrink" => format!(".flex_shrink({})", format_number(value, scope)?),
        "span" => format!(".grid_column_span({})", track_count(key, value)?),
        "row_span" => format!(".grid_row_span({})", track_count(key, value)?),
        "cols" => {
            let tracks = parse_grid_template(value).ok_or_else(|| {
                format!(
                    "`cols:{value}` is not a track list: write a count (`cols:3`), sizes (`cols(240 1fr auto)`), or `cols(fill 260)`"
                )
            })?;
            format!(".display_grid().grid_template_columns(vec![{tracks}])")
        }
        "axis" => format!(".{}()", keyword(key, value, registry::AXIS_VALUES)?),
        "align" => format!(
            ".align_items({})",
            keyword(key, value, registry::ALIGN_VALUES)?
        ),
        "justify" => format!(
            ".justify_content({})",
            keyword(key, value, registry::JUSTIFY_VALUES)?
        ),
        // Visual-only props are applied at node level, not in LayoutStyle.
        _ => return Ok(None),
    };
    Ok(Some(call))
}

/// The Rust name `value` generates under `key`, or the message naming what the key does accept.
pub fn keyword(
    key: &str,
    value: &str,
    table: &'static [(&'static str, &'static str)],
) -> Result<&'static str, String> {
    registry::keyword(table, value).ok_or_else(|| {
        let spellings: Vec<String> = table
            .iter()
            .filter(|(name, _)| !name.is_empty())
            .map(|(name, _)| format!("`{name}`"))
            .collect();
        let expected = match spellings.split_last() {
            Some((last, [])) => last.clone(),
            Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
            None => String::new(),
        };
        let flag = table.iter().any(|(name, _)| name.is_empty());
        if spellings.is_empty() {
            return format!("`{key}` takes no value: writing it is the assertion itself");
        }
        let bare = if flag { ", or the bare flag" } else { "" };
        match value.is_empty() {
            true => format!("`{key}` needs a value: expected {expected}"),
            false => {
                format!("`{key}:{value}` is not a value of `{key}`: expected {expected}{bare}")
            }
        }
    })
}

fn track_count(key: &str, value: &str) -> Result<u16, String> {
    value
        .parse::<u16>()
        .map_err(|_| format!("`{key}:{value}` is not a whole number of grid tracks"))
}

/// Parses a `cols` value into a comma-separated `TemplateTrack` expression list. `"3"` → `repeat(3, 1fr)`; `"1fr 2fr"` → individual tracks; `"fill 260"` / `"fit 260"` → `repeat(auto-fill|auto-fit, minmax(260px, 1fr))` — a responsive grid that reflows like flex-wrap but, unlike it, reports a correct height when nested in another container.
fn parse_grid_template(value: &str) -> Option<String> {
    let s = value.trim();
    let tokens: Vec<&str> = s.split_whitespace().collect();
    if let [kind @ ("fill" | "fit"), min] = tokens.as_slice() {
        let min_px = min.parse::<f32>().ok()?;
        let repeat = if *kind == "fill" { "fill" } else { "fit" };
        return Some(format!(
            "TemplateTrack::{repeat}(TemplateTrack::minmax(TemplateTrack::px({}), TemplateTrack::fr(1.0)))",
            format_f32(min_px)
        ));
    }
    if let Ok(n) = s.parse::<u32>() {
        return Some(format!(
            "TemplateTrack::repeat({n}, TemplateTrack::fr(1.0))"
        ));
    }
    let tracks: Option<Vec<String>> = s.split_whitespace().map(parse_track_token).collect();
    tracks.map(|v| v.join(", "))
}

/// One track of a `cols(…)` list. `fr` earns its suffix — it is a real unit with no other spelling — where
/// `px` was the implicit unit everywhere else in the language and a second spelling of a bare number here.
fn parse_track_token(s: &str) -> Option<String> {
    if let Some(rest) = s.strip_suffix("fr") {
        let n: f32 = rest.parse().ok()?;
        return Some(format!("TemplateTrack::fr({})", format_f32(n)));
    }
    if s == "auto" {
        return Some("TemplateTrack::auto()".to_string());
    }
    let n: f32 = s.parse().ok()?;
    Some(format!("TemplateTrack::px({})", format_f32(n)))
}

/// Resolves a numeric value to the Rust expression it stands for, or reports why it is not a number.
///
/// Everything the markup can resolve on its own resolves here — a literal, a `$signal`, a `theme.…` read, a
/// `[style]` constant — and everything it cannot is either a name the author has in scope (carried through
/// for rustc to look up, on this attribute's own line) or a typo, which is the case this returns an error
/// for. Before, a typo was emitted as Rust verbatim and reported by rustc against generated code the author
/// never wrote.
pub fn format_number(value: &str, scope: Scope<'_>) -> Result<String, String> {
    let v = value.trim();
    // A `$signal` sizes the node from reactive state. The read is emitted inline, which is correct in both
    // places the style expression lands: once at construction, and again inside the `styled_by` effect the
    // container grows when any of its layout props is reactive.
    if let Some(read) = signal_read(v) {
        return Ok(read);
    }
    if let Some(expr) = theme_field_expr(v, scope.theme) {
        return Ok(expr);
    }
    if let Ok(n) = v.parse::<f32>() {
        return Ok(format_f32(n));
    }
    if v.contains('(') && balanced(v) {
        return Ok(v.to_string());
    }
    // A binding shadows a `[style]` constant of the same name, as in Rust and as `color_expr` already does.
    if scope.locals.iter().any(|local| local == v) {
        return Ok(v.to_string());
    }
    match scope.constant(v) {
        Some(StyleValue::Number(_)) => return Ok(constant_name("SIZE_", v)),
        Some(StyleValue::Hex(_)) => {
            return Err(format!(
                "`{v}` is declared in `[style]` as a colour, not a number"
            ));
        }
        Some(StyleValue::Raw(_)) => {
            return Err(format!(
                "`{v}` is declared in `[style]` as text, not a number"
            ));
        }
        None => {}
    }
    if is_path_expr(v) {
        return Ok(v.to_string());
    }
    Err(format!(
        "`{v}` is not a number: write a literal, a `$signal`, a `theme.…` read, a `[style]` constant, or a parenthesised expression"
    ))
}

/// `$name` -> `name.get()`, for a lone signal identifier. Anything more complex is left to the caller.
pub fn signal_read(value: &str) -> Option<String> {
    let ident = value.trim().strip_prefix('$')?;
    let mut chars = ident.chars();
    let head_ok = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    let tail_ok = chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    (head_ok && tail_ok).then(|| format!("{ident}.get()"))
}

/// Renders a sizing value: `%` becomes `SizeDimension::Percent` (`100%` == `1.0`); everything else is a
/// number, resolved by [`format_number`].
pub fn dimension(value: &str, scope: Scope<'_>) -> Result<String, String> {
    let v = value.trim();
    if let Some(pct) = v.strip_suffix('%') {
        return match pct.trim().parse::<f32>() {
            Ok(n) => Ok(format!("SizeDimension::Percent({})", format_f32(n / 100.0))),
            Err(_) => Err(format!("`{v}` is not a percentage")),
        };
    }
    format_number(v, scope)
}

/// Formats an f32 so it always carries a decimal point (`240` -> `240.0`).
pub fn format_f32(n: f32) -> String {
    if n.fract() == 0.0 {
        format!("{n:.1}")
    } else {
        let s = format!("{n}");
        if s.contains('.') { s } else { format!("{s}.0") }
    }
}

/// Builds a `Color::rgba(...)` const expression from a hex string, at any length
/// [`telar_parser::parse_hex`] accepts. Anything else falls back to opaque black, which the parser has
/// already rejected before a value reaches here.
pub fn hex_to_color_expr(hex: &str) -> String {
    let [r, g, b, a] = telar_parser::parse_hex(hex).unwrap_or([0, 0, 0, 255]);
    format!("Color::rgba({r}.0 / 255.0, {g}.0 / 255.0, {b}.0 / 255.0, {a}.0 / 255.0)")
}

#[cfg(test)]
mod tests {
    use super::*;
    use telar_parser::StyleProp;

    fn constants(entries: &[(&str, StyleValue)]) -> Vec<StyleConstant> {
        entries
            .iter()
            .map(|(name, value)| StyleConstant {
                name: name.to_string(),
                value: value.clone(),
                line: 1,
            })
            .collect()
    }

    fn call(key: &str, value: &str) -> Option<String> {
        match layout_prop_call(key, value, Scope::default()) {
            PropCall::Call(call) => Some(call),
            _ => None,
        }
    }

    fn invalid(key: &str, value: &str) -> Option<String> {
        match layout_prop_call(key, value, Scope::default()) {
            PropCall::Invalid(message) => Some(message),
            _ => None,
        }
    }

    fn themed(key: &str, value: &str, theme: &str) -> Option<String> {
        let scope = Scope {
            theme: Some(theme),
            ..Scope::default()
        };
        match layout_prop_call(key, value, scope) {
            PropCall::Call(call) => Some(call),
            _ => None,
        }
    }

    #[test]
    fn hex_expands() {
        assert_eq!(
            hex_to_color_expr("#3d78fa"),
            "Color::rgba(61.0 / 255.0, 120.0 / 255.0, 250.0 / 255.0, 255.0 / 255.0)"
        );
    }

    #[test]
    fn width_prop() {
        assert_eq!(call("width", "240").as_deref(), Some(".width(240.0)"));
    }

    #[test]
    fn logical_edge_props_map_to_their_builder_calls() {
        for (key, expected) in [
            ("pad_start", ".padding_start(12.0)"),
            ("padding_start", ".padding_start(12.0)"),
            ("pad_end", ".padding_end(12.0)"),
            ("margin_start", ".margin_inline_start(12.0)"),
            ("inset_end", ".inset_end(12.0)"),
        ] {
            assert_eq!(call(key, "12").as_deref(), Some(expected), "{key}");
        }
    }

    #[test]
    fn direction_row_reverse_is_the_physical_one() {
        assert_eq!(call("axis", "row").as_deref(), Some(".flex_row()"));
        assert_eq!(
            call("axis", "row_reverse").as_deref(),
            Some(".flex_row_reverse()")
        );
    }

    #[test]
    fn a_theme_path_resolves_in_any_numeric_prop() {
        for (key, expected) in [
            ("pad", ".padding_all(use_theme::<Th>().gutter)"),
            ("gap", ".gap(use_theme::<Th>().gutter)"),
            ("width", ".width(use_theme::<Th>().gutter)"),
            (
                "margin_start",
                ".margin_inline_start(use_theme::<Th>().gutter)",
            ),
        ] {
            assert_eq!(
                themed(key, "theme.gutter", "Th").as_deref(),
                Some(expected),
                "{key}"
            );
        }
    }

    #[test]
    fn a_bare_ident_still_means_a_style_constant_not_a_theme_field() {
        // The whole reason the theme form is dotted: this must keep meaning what it always meant.
        assert_eq!(
            themed("pad", "card_gap", "Th").as_deref(),
            Some(".padding_all(card_gap)")
        );
    }

    #[test]
    fn a_theme_path_without_a_theme_is_left_for_rustc() {
        assert_eq!(theme_field_expr("theme.gutter", None), None);
        assert_eq!(theme_field_expr("gutter", Some("Th")), None);
        assert_eq!(theme_field_expr("theme.", Some("Th")), None);
        assert_eq!(theme_field_expr("theme.not an ident", Some("Th")), None);
    }

    /// A theme's vocabulary is the application's, not the DSL's: half of what a real theme answers is a method
    /// with an argument, and until this every one of those had to be hoisted into `[logic]` and given a name
    /// the view then used instead.
    #[test]
    fn a_theme_read_can_be_a_call_or_a_chain_not_only_a_field() {
        assert_eq!(
            theme_field_expr("theme.font(FontRole::Body)", Some("Th")).as_deref(),
            Some("use_theme::<Th>().font(FontRole::Body)")
        );
        assert_eq!(
            theme_field_expr("theme.accent.darken(0.1)", Some("Th")).as_deref(),
            Some("use_theme::<Th>().accent.darken(0.1)")
        );
        // Half-written, so it is left for the arm that would have handled it rather than emitted as a read.
        assert_eq!(
            theme_field_expr("theme.font(FontRole::Body", Some("Th")),
            None
        );
    }

    #[test]
    fn radius_is_ignored() {
        assert!(matches!(
            layout_prop_call("radius", "6", Scope::default()),
            PropCall::Other
        ));
    }

    #[test]
    fn aspect_maps_to_aspect_ratio() {
        assert_eq!(call("aspect", "1").as_deref(), Some(".aspect_ratio(1.0)"));
        assert_eq!(
            call("aspect_ratio", "1.5").as_deref(),
            Some(".aspect_ratio(1.5)")
        );
    }

    /// The read half of `[style]` numeric constants, which was never written: `generate_constant` has always
    /// emitted `const SIZE_*` and nothing has ever resolved a name to one, so every one of them was dead.
    #[test]
    fn a_numeric_style_constant_resolves_to_the_const_it_generates() {
        let declared = constants(&[("card_gap", StyleValue::Number(6.0))]);
        let scope = Scope {
            constants: &declared,
            ..Scope::default()
        };
        assert_eq!(format_number("card_gap", scope).unwrap(), "SIZE_CARD_GAP");
        assert!(matches!(
            layout_prop_call("gap", "card_gap", scope),
            PropCall::Call(ref c) if c == ".gap(SIZE_CARD_GAP)"
        ));
    }

    /// A `[logic]` binding shadows a `[style]` constant of the same name, the way a local shadows in Rust —
    /// and the way `color_expr` already resolves a colour.
    #[test]
    fn a_logic_binding_shadows_a_style_constant_of_the_same_name() {
        let declared = constants(&[("gutter", StyleValue::Number(6.0))]);
        let locals = vec!["gutter".to_string()];
        let scope = Scope {
            constants: &declared,
            locals: &locals,
            ..Scope::default()
        };
        assert_eq!(format_number("gutter", scope).as_deref(), Ok("gutter"));
    }

    /// A name the author did declare, under a key that cannot use it — which used to be a rustc error about
    /// a `SIZE_*` symbol nobody wrote.
    #[test]
    fn a_constant_of_the_wrong_kind_says_which_kind_it_is() {
        let declared = constants(&[
            ("brand", StyleValue::Hex("#3d78fa".into())),
            ("label", StyleValue::Raw("hello".into())),
        ]);
        let scope = Scope {
            constants: &declared,
            ..Scope::default()
        };
        assert!(
            format_number("brand", scope)
                .unwrap_err()
                .contains("as a colour, not a number")
        );
        assert!(
            format_number("label", scope)
                .unwrap_err()
                .contains("as text, not a number")
        );
    }

    /// T6: a bare token under a numeric key used to be emitted as Rust verbatim, so `gap:1O` became `.gap(1O)`
    /// and rustc complained about generated code. A name is still carried through — a `[logic]` binding, a
    /// `const`, a props field are all names this crate cannot enumerate — but a value that is not a name at
    /// all can only be a typo.
    #[test]
    fn a_value_that_is_not_a_name_is_rejected_instead_of_emitted_as_rust() {
        for value in ["1O", "12px", "10 px", "6,"] {
            assert!(
                invalid("gap", value).is_some(),
                "`gap:{value}` should not compile"
            );
        }
        for value in ["side", "props.pad", "crate::scale::md()", "TRACK_H"] {
            assert!(
                call("gap", value).is_some(),
                "`gap:{value}` names something the author has in scope"
            );
        }
    }

    /// A percentage falls where the key has no meaning for one, rather than through the old escape into a
    /// rustc error about `50%` in generated Rust.
    #[test]
    fn a_percentage_is_a_size_and_a_number_is_everything_else() {
        assert_eq!(
            call("width", "50%").as_deref(),
            Some(".width(SizeDimension::Percent(0.5))")
        );
        assert!(invalid("pad", "50%").is_some());
    }

    /// S3: a value outside a closed keyword set now says what the set is, on the attribute, instead of the
    /// property being dropped and the layout coming out subtly wrong.
    #[test]
    fn an_unknown_keyword_names_the_set_it_is_not_in() {
        let message = invalid("align", "centre").expect("`align:centre` should not compile");
        assert!(message.contains("`align:centre` is not a value of `align`"));
        assert!(message.contains("`center`") && message.contains("`stretch`"));
        assert!(invalid("justify", "middle").is_some());
        assert!(invalid("axis", "sideways").is_some());
        assert!(invalid("self", "middle").is_some());
    }

    /// A flag key still takes its bare form, and still rejects a spelled-out value it does not have.
    #[test]
    fn a_flag_key_keeps_its_bare_form() {
        assert_eq!(call("wrap", "").as_deref(), Some(".flex_wrap()"));
        assert_eq!(call("absolute", "").as_deref(), Some(".absolute()"));
        assert_eq!(
            call("absolute", "fill").as_deref(),
            Some(".absolute_fill()")
        );
        let message = invalid("absolute", "middle").expect("`absolute:middle` should not compile");
        assert!(message.contains("the bare flag"), "{message}");
    }

    /// All three `[style]` value kinds are real items now. `Raw` used to be emitted as a comment saying it had
    /// "no obvious Rust target type", which left one of the three declarable kinds inert.
    #[test]
    fn every_style_constant_kind_emits_a_typed_item() {
        let section = StyleSection {
            constants: constants(&[
                ("primary", StyleValue::Hex("#3d78fa".into())),
                ("radius", StyleValue::Number(6.0)),
                ("label", StyleValue::Raw("hello".into())),
            ]),
            classes: Vec::new(),
        };
        let out = generate_style_section(&section, None);
        assert!(out.contains("const COLOR_PRIMARY: Color = Color::rgba("));
        assert!(out.contains("const SIZE_RADIUS: f32 = 6.0;"));
        assert!(out.contains(r#"const RAW_LABEL: &str = "hello";"#));
        assert!(!out.contains("unmapped"));
    }

    /// A class property is written where no element is, so it has no attribute line — but it is the same
    /// misspelling, and it used to be dropped just as silently.
    #[test]
    fn a_class_property_reports_its_own_bad_value() {
        let section = StyleSection {
            constants: Vec::new(),
            classes: vec![StyleClass {
                name: "card".into(),
                props: vec![StyleProp {
                    key: "align".into(),
                    value: "centre".into(),
                }],
                line: 1,
            }],
        };
        let out = generate_style_section(&section, None);
        assert!(out.contains("compile_error!"));
        assert!(out.contains("in `@card`"), "{out}");
    }

    /// A class property nobody recognises used to be dropped on the floor, which is how a renamed key goes
    /// on compiling and quietly stops laying the class out. Element attributes have been checked this way
    /// since keys were checked at all; classes never were.
    #[test]
    fn a_class_property_with_an_unknown_key_is_rejected() {
        let section = StyleSection {
            constants: Vec::new(),
            classes: vec![StyleClass {
                name: "card".into(),
                props: vec![StyleProp {
                    key: "direction".into(),
                    value: "col".into(),
                }],
                line: 1,
            }],
        };
        let out = generate_style_section(&section, None);
        assert!(out.contains("`direction` is not a style property"), "{out}");
    }

    /// A paint key is not a layout property and must not be mistaken for an unknown one: it reaches the
    /// `RectStyle` by another path entirely.
    #[test]
    fn a_paint_property_in_a_class_is_not_mistaken_for_an_unknown_key() {
        let section = StyleSection {
            constants: Vec::new(),
            classes: vec![StyleClass {
                name: "card".into(),
                props: vec![StyleProp {
                    key: "fill".into(),
                    value: "#fff".into(),
                }],
                line: 1,
            }],
        };
        assert!(!generate_style_section(&section, None).contains("compile_error!"));
    }
}
