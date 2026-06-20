//! Generates the `[style]` section: color/number constants and per-class
//! `LayoutStyle` constructor functions.

use std::fmt::Write;

use rsx_parser::{StyleClass, StyleConst, StyleSection, StyleValue};

use crate::naming::{const_name, style_fn_name};

/// Renders all constants and style functions for the document's style section.
/// When `skip_color_consts` is set (a theme type is active), color constants are
/// omitted because color references resolve through `use_theme` instead.
pub fn generate_style_section(section: &StyleSection, skip_color_consts: bool) -> String {
    let mut out = String::new();

    let mut emitted_const = false;
    for c in &section.constants {
        if skip_color_consts && matches!(c.value, StyleValue::Hex(_)) {
            continue;
        }
        out.push_str(&generate_const(c));
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
        out.push_str(&generate_class_fn(class));
        out.push('\n');
    }

    out
}

fn generate_const(c: &StyleConst) -> String {
    match &c.value {
        StyleValue::Hex(hex) => {
            let name = const_name("COLOR_", &c.name);
            format!("const {name}: Color = {};", hex_to_color_expr(hex))
        }
        StyleValue::Number(n) => {
            let name = const_name("SIZE_", &c.name);
            format!("const {name}: f32 = {};", format_f32(*n))
        }
        // Raw constants have no obvious Rust target type, so we leave a note
        // rather than emit something that fails to compile.
        StyleValue::Raw(raw) => {
            let name = const_name("RAW_", &c.name);
            format!("// raw style constant `{}` = {raw:?} (unmapped)", name)
        }
    }
}

fn generate_class_fn(class: &StyleClass) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "fn {}() -> LayoutStyle {{", style_fn_name(&class.name));
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
        "width" => format!(".width({})", num(value)),
        "height" => format!(".height({})", num(value)),
        "min-width" => format!(".min_width({})", num(value)),
        "min-height" => format!(".min_height({})", num(value)),
        "padding" => format!(".padding_all({})", num(value)),
        "padding-x" => format!(".padding_horizontal({})", num(value)),
        "padding-y" => format!(".padding_vertical({})", num(value)),
        "gap" => format!(".gap({})", num(value)),
        "gap-x" => format!(".gap_x({})", num(value)),
        "gap-y" => format!(".gap_y({})", num(value)),
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

/// Renders a numeric literal as a float suffix-free Rust expression. Non-numeric
/// values are passed through verbatim (e.g. references to other constants).
fn num(value: &str) -> String {
    match value.parse::<f32>() {
        Ok(n) => format_f32(n),
        Err(_) => value.to_string(),
    }
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
