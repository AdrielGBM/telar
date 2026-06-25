//! Generates the `[style]` section: color/number constants and per-class
//! `LayoutStyle` constructor functions.

use std::fmt::Write;

use rsx_parser::{StyleClass, StyleConstant, StyleSection, StyleValue};

use crate::naming::{constant_name, style_function_name};

/// Renders all constants and style functions for the document's style section.
/// When a theme is active (`theme_active`), `[style]` color constants are omitted: color references resolve through `use_theme` instead (see `color_expr`), so they react to theme switches. Number/raw constants are always emitted.
pub fn generate_style_section(section: &StyleSection, theme_active: bool) -> String {
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
        out.push_str(&generate_class_function(class));
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

fn generate_class_function(class: &StyleClass) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "fn {}() -> LayoutStyle {{",
        style_function_name(&class.name)
    );
    out.push_str("    LayoutStyle::new()");
    for prop in &class.props {
        if let Some(call) = layout_prop_call(&prop.key, &prop.value) {
            let _ = write!(out, "\n        {call}");
        }
    }
    out.push_str("\n}");
    out
}

/// Maps a style property to a `LayoutStyle` builder call, or `None` if the
/// property is purely visual and not represented in layout.
pub fn layout_prop_call(key: &str, value: &str) -> Option<String> {
    let value = value.trim();
    Some(match key {
        "width" => format!(".width({})", dimension(value)),
        "height" => format!(".height({})", dimension(value)),
        "min-width" => format!(".min_width({})", dimension(value)),
        "min-height" => format!(".min_height({})", dimension(value)),
        "max-width" => format!(".max_width({})", dimension(value)),
        "max-height" => format!(".max_height({})", dimension(value)),
        "basis" | "flex-basis" => format!(".flex_basis({})", dimension(value)),
        // `wrap` is a flag (no value) or `wrap`/`true`; anything else is ignored.
        "wrap" => match value {
            "" | "wrap" | "true" => ".flex_wrap()".to_string(),
            _ => return None,
        },
        // Per-child cross-axis stretch, e.g. `self:stretch` to override a parent `align:center`.
        "self" => match value {
            "stretch" => ".align_self_stretch()".to_string(),
            _ => return None,
        },
        "padding" | "pad" => format!(".padding_all({})", format_number(value)),
        "padding-x" | "pad-x" => format!(".padding_horizontal({})", format_number(value)),
        "padding-y" | "pad-y" => format!(".padding_vertical({})", format_number(value)),
        "gap" => format!(".gap({})", format_number(value)),
        "gap-x" => format!(".gap_x({})", format_number(value)),
        "gap-y" => format!(".gap_y({})", format_number(value)),
        "grow" => format!(".flex_grow({})", format_number(value)),
        "shrink" => format!(".flex_shrink({})", format_number(value)),
        "span" => match value.trim().parse::<u16>() {
            Ok(n) => format!(".grid_column_span({n})"),
            Err(_) => return None,
        },
        "row-span" => match value.trim().parse::<u16>() {
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

/// Parses a `cols` value into a comma-separated `TemplateTrack` expression list.
/// `"3"` → `repeat(3, 1fr)`; `"1fr 2fr"` → individual tracks;
/// `"fill 260"` / `"fit 260"` → `repeat(auto-fill|auto-fit, minmax(260px, 1fr))`
/// — a responsive grid that reflows like flex-wrap but, unlike it, reports a
/// correct height when nested in another container.
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

/// Renders a numeric literal as a float suffix-free Rust expression. Non-numeric
/// values are passed through verbatim (e.g. references to other constants).
fn format_number(value: &str) -> String {
    match value.parse::<f32>() {
        Ok(n) => format_f32(n),
        Err(_) => value.to_string(),
    }
}

/// Renders a sizing value for `width`/`height`/`min-*`/`max-*`/`basis`. A `%`
/// suffix becomes `SizeDimension::Percent` (where `100%` == `1.0`); a bare
/// number stays an `f32` literal (coerced to `Px` via `Into<SizeDimension>`),
/// and anything else is forwarded verbatim (e.g. a `[style]` constant name).
fn dimension(value: &str) -> String {
    let v = value.trim();
    if let Some(pct) = v.strip_suffix('%') {
        if let Ok(n) = pct.trim().parse::<f32>() {
            return format!("SizeDimension::Percent({})", format_f32(n / 100.0));
        }
    }
    format_number(v)
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

/// Builds a `Color::rgba(...)` const expression from a `#rrggbb` / `#rrggbbaa` string.
pub fn hex_to_color_expr(hex: &str) -> String {
    let h = hex.trim_start_matches('#');
    let parse = |s: &str| u8::from_str_radix(s, 16).unwrap_or(0);
    let (r, g, b, a) = match h.len() {
        6 => (parse(&h[0..2]), parse(&h[2..4]), parse(&h[4..6]), 255),
        8 => (
            parse(&h[0..2]),
            parse(&h[2..4]),
            parse(&h[4..6]),
            parse(&h[6..8]),
        ),
        3 => {
            let dup = |c: &str| parse(&format!("{c}{c}"));
            (dup(&h[0..1]), dup(&h[1..2]), dup(&h[2..3]), 255)
        }
        _ => (0, 0, 0, 255),
    };
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
            layout_prop_call("width", "240").as_deref(),
            Some(".width(240.0)")
        );
    }

    #[test]
    fn radius_is_ignored() {
        assert!(layout_prop_call("radius", "6").is_none());
    }
}
