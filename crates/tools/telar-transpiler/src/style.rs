//! Generates the `[style]` section: color/number constants and per-class `LayoutStyle` constructor functions.

use std::fmt::Write;

use telar_parser::{StyleClass, StyleConstant, StyleSection, StyleValue};

use crate::naming::{constant_name, is_ident, style_function_name, to_snake_case};

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

    for (i, class) in section.classes.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&generate_class_function(class, theme));
        out.push('\n');
    }

    out
}

fn generate_constant(c: &StyleConstant) -> String {
    match &c.value {
        StyleValue::Hex(hex) => {
            let name = constant_name("COLOR_", &c.name);
            format!("const {name}: Color = {};", hex_to_color_expr(hex))
        }
        StyleValue::Number(n) => {
            let name = constant_name("SIZE_", &c.name);
            format!("const {name}: f32 = {};", format_f32(*n))
        }
        // Raw constants have no obvious Rust target type, so we leave a note rather than emit something that fails to compile.
        StyleValue::Raw(raw) => {
            let name = constant_name("RAW_", &c.name);
            format!("// raw style constant `{}` = {raw:?} (unmapped)", name)
        }
    }
}

fn generate_class_function(class: &StyleClass, theme: Option<&str>) -> String {
    let mut out = String::new();
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
        if let Some(call) = layout_prop_call(&prop.key, &prop.value, theme) {
            let _ = write!(out, "\n        {call}");
        }
    }
    out.push_str("\n}");
    out
}

/// Maps a style property to a `LayoutStyle` builder call, or `None` if the property is purely visual and not represented in layout.
pub fn layout_prop_call(key: &str, value: &str, theme: Option<&str>) -> Option<String> {
    let value = value.trim();
    Some(match key {
        "width" => format!(".width({})", dimension(value, theme)),
        "height" => format!(".height({})", dimension(value, theme)),
        "min_width" => format!(".min_width({})", dimension(value, theme)),
        "min_height" => format!(".min_height({})", dimension(value, theme)),
        "max_width" => format!(".max_width({})", dimension(value, theme)),
        "max_height" => format!(".max_height({})", dimension(value, theme)),
        "basis" | "flex_basis" => format!(".flex_basis({})", dimension(value, theme)),
        "aspect" | "aspect_ratio" => format!(".aspect_ratio({})", format_number(value, theme)),
        // `wrap` is a flag (no value) or `wrap`/`true`; anything else is ignored.
        "wrap" => match value {
            "" | "wrap" | "true" => ".flex_wrap()".to_string(),
            _ => return None,
        },
        // Per-child cross-axis alignment override, e.g. `self:stretch` over a parent `align:center`, or
        // `self:center` to keep a fixed-size child centered instead of stretched.
        "self" => match value {
            "stretch" => ".align_self_stretch()".to_string(),
            "center" => ".align_self_center()".to_string(),
            "start" => ".align_self_start()".to_string(),
            "end" => ".align_self_end()".to_string(),
            _ => return None,
        },
        "padding" | "pad" => format!(".padding_all({})", format_number(value, theme)),
        "padding_x" | "pad_x" => format!(".padding_horizontal({})", format_number(value, theme)),
        "padding_y" | "pad_y" => format!(".padding_vertical({})", format_number(value, theme)),
        // Logical edges: resolved to left/right against the active writing direction at layout time, so one build serves LTR and RTL.
        "padding_start" | "pad_start" => format!(".padding_start({})", format_number(value, theme)),
        "padding_end" | "pad_end" => format!(".padding_end({})", format_number(value, theme)),
        "margin_start" => format!(".margin_inline_start({})", format_number(value, theme)),
        "margin_end" => format!(".margin_inline_end({})", format_number(value, theme)),
        "inset_start" => format!(".inset_start({})", format_number(value, theme)),
        "inset_end" => format!(".inset_end({})", format_number(value, theme)),
        "inset_top" => format!(".inset_top({})", format_number(value, theme)),
        "inset_bottom" => format!(".inset_bottom({})", format_number(value, theme)),
        // Out of flow, pinned only by the insets the author names. `absolute_fill` is the all-four-at-zero
        // shorthand `overlay` uses; a floating panel wants three edges and its own size on the fourth.
        "absolute" => match value {
            "" | "true" => ".absolute()".to_string(),
            "fill" => ".absolute_fill()".to_string(),
            _ => return None,
        },
        "gap" => format!(".gap({})", format_number(value, theme)),
        "gap_x" => format!(".gap_x({})", format_number(value, theme)),
        "gap_y" => format!(".gap_y({})", format_number(value, theme)),
        "grow" => format!(".flex_grow({})", format_number(value, theme)),
        "shrink" => format!(".flex_shrink({})", format_number(value, theme)),
        "span" => match value.trim().parse::<u16>() {
            Ok(n) => format!(".grid_column_span({n})"),
            Err(_) => return None,
        },
        "row_span" => match value.trim().parse::<u16>() {
            Ok(n) => format!(".grid_row_span({n})"),
            Err(_) => return None,
        },
        "cols" => match parse_grid_template(value) {
            Some(tracks) => format!(".display_grid().grid_template_columns(vec![{tracks}])"),
            None => return None,
        },
        "direction" => match value {
            "col" | "column" => ".flex_column()".to_string(),
            "row" => ".flex_row()".to_string(),
            // Reversed in both writing directions, unlike `row`, which follows the active one.
            "row_reverse" => ".flex_row_reverse()".to_string(),
            _ => return None,
        },
        "align" => format!(".align_items(AlignItems::{})", align_variant(value)?),
        "justify" => format!(
            ".justify_content(JustifyContent::{})",
            justify_variant(value)?
        ),
        // Visual-only props are applied at node level, not in LayoutStyle.
        _ => return None,
    })
}

fn align_variant(value: &str) -> Option<&'static str> {
    Some(match value {
        "center" => "CENTER",
        "start" => "START",
        "end" => "END",
        "stretch" => "STRETCH",
        "flex-start" => "FLEX_START",
        "flex-end" => "FLEX_END",
        _ => return None,
    })
}

fn justify_variant(value: &str) -> Option<&'static str> {
    Some(match value {
        "center" => "CENTER",
        "start" => "START",
        "end" => "END",
        "between" | "space-between" => "SPACE_BETWEEN",
        "around" | "space-around" => "SPACE_AROUND",
        "evenly" | "space-evenly" => "SPACE_EVENLY",
        _ => return None,
    })
}

/// Parses a `cols` value into a comma-separated `TemplateTrack` expression list. `"3"` → `repeat(3, 1fr)`; `"1fr 2fr"` → individual tracks; `"fill 260"` / `"fit 260"` → `repeat(auto-fill|auto-fit, minmax(260px, 1fr))` — a responsive grid that reflows like flex-wrap but, unlike it, reports a correct height when nested in another container.
fn parse_grid_template(value: &str) -> Option<String> {
    let s = value.trim();
    let tokens: Vec<&str> = s.split_whitespace().collect();
    if let [kind @ ("fill" | "fit"), min] = tokens.as_slice() {
        let min_px = min.trim_end_matches("px").parse::<f32>().ok()?;
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

fn parse_track_token(s: &str) -> Option<String> {
    if let Some(rest) = s.strip_suffix("fr") {
        let n: f32 = rest.parse().ok()?;
        return Some(format!("TemplateTrack::fr({})", format_f32(n)));
    }
    if let Some(rest) = s.strip_suffix("px") {
        let n: f32 = rest.parse().ok()?;
        return Some(format!("TemplateTrack::px({})", format_f32(n)));
    }
    if s == "auto" {
        return Some("TemplateTrack::auto()".to_string());
    }
    let n: f32 = s.parse().ok()?;
    Some(format!("TemplateTrack::px({})", format_f32(n)))
}

/// Renders a numeric literal as a float suffix-free Rust expression. Non-numeric values are passed through verbatim (e.g. references to other constants).
pub fn format_number(value: &str, theme: Option<&str>) -> String {
    // A `$signal` sizes the node from reactive state. The read is emitted inline, which is correct in both
    // places the style expression lands: once at construction, and again inside the `styled_by` effect the
    // container grows when any of its layout props is reactive.
    if let Some(read) = signal_read(value) {
        return read;
    }
    if let Some(expr) = theme_field_expr(value, theme) {
        return expr;
    }
    match value.parse::<f32>() {
        Ok(n) => format_f32(n),
        Err(_) => value.to_string(),
    }
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

/// Renders a sizing value: `%` becomes `SizeDimension::Percent` (`100%` == `1.0`), a bare number stays an
/// `f32` literal (coerced to `Px`), and anything else is forwarded verbatim (e.g. a `[style]` constant name).
fn dimension(value: &str, theme: Option<&str>) -> String {
    let v = value.trim();
    if let Some(pct) = v.strip_suffix('%') {
        if let Ok(n) = pct.trim().parse::<f32>() {
            return format!("SizeDimension::Percent({})", format_f32(n / 100.0));
        }
    }
    format_number(v, theme)
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

    #[test]
    fn hex_expands() {
        assert_eq!(
            hex_to_color_expr("#3d78fa"),
            "Color::rgba(61.0 / 255.0, 120.0 / 255.0, 250.0 / 255.0, 255.0 / 255.0)"
        );
    }

    #[test]
    fn width_prop() {
        assert_eq!(
            layout_prop_call("width", "240", None).as_deref(),
            Some(".width(240.0)")
        );
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
            assert_eq!(
                layout_prop_call(key, "12", None).as_deref(),
                Some(expected),
                "{key}"
            );
        }
    }

    #[test]
    fn direction_row_reverse_is_the_physical_one() {
        assert_eq!(
            layout_prop_call("direction", "row", None).as_deref(),
            Some(".flex_row()")
        );
        assert_eq!(
            layout_prop_call("direction", "row_reverse", None).as_deref(),
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
                layout_prop_call(key, "theme.gutter", Some("Th")).as_deref(),
                Some(expected),
                "{key}"
            );
        }
    }

    #[test]
    fn a_bare_ident_still_means_a_style_constant_not_a_theme_field() {
        // The whole reason the theme form is dotted: this must keep meaning what it always meant.
        assert_eq!(
            layout_prop_call("pad", "card_gap", Some("Th")).as_deref(),
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
        assert!(layout_prop_call("radius", "6", None).is_none());
    }

    #[test]
    fn aspect_maps_to_aspect_ratio() {
        assert_eq!(
            layout_prop_call("aspect", "1", None).as_deref(),
            Some(".aspect_ratio(1.0)")
        );
        assert_eq!(
            layout_prop_call("aspect_ratio", "1.5", None).as_deref(),
            Some(".aspect_ratio(1.5)")
        );
    }
}
