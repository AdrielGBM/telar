//! Generates the `[style]` section: one `LayoutStyle` constructor function per class.

use std::fmt::Write;

use telar_parser::{StyleClass, StyleSection};

use crate::naming::style_function_name;
use crate::registry;

/// A number for a key [`crate::registry::value_kind`] describes, where a value the key cannot mean has already been reported on the attribute itself. What stands in its place only has to be *something*: the build stops before anything reads it.
pub fn number_or(value: &str, fallback: &str) -> String {
    format_number(value).unwrap_or_else(|_| fallback.to_string())
}

/// A number in a position with no attribute of its own to carry a diagnostic — a nested `hover_style(…)` property — where the error has to travel inside the expression or not at all.
pub fn number_or_error(value: &str) -> String {
    format_number(value)
        .unwrap_or_else(|message| format!("compile_error!({})", crate::view::rust_str(&message)))
}

/// What a `key:value` attribute contributes to a `LayoutStyle`.
pub enum PropCall {
    /// The builder call to chain on.
    Call(String),
    /// Not a layout property — a paint or behaviour key its own emitter handles.
    Other,
    /// A value this key cannot mean, and why. Reported on the attribute rather than dropped, which is the whole difference between a misspelled key and a misspelled value having a diagnostic.
    Invalid(String),
}

/// Renders one `LayoutStyle` constructor function per class in the document's style section.
pub fn generate_style_section(section: &StyleSection, theme: Option<&str>) -> String {
    let mut out = String::new();
    for (i, class) in section.classes.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&generate_class_function(class, theme));
        out.push('\n');
    }
    out
}

/// What a class may carry beyond the layout keys: what a box paints with, and what flows down to the text below it. Anything else is a typo, and used to be dropped on the floor.
fn is_style_key(key: &str) -> bool {
    crate::view::is_paint_key(key)
        || crate::registry::INHERITABLE_TEXT_KEYS.contains(&key)
        || matches!(key, "lines" | "ellipsis")
}

fn generate_class_function(class: &StyleClass, theme: Option<&str>) -> String {
    let mut out = String::new();
    // A class property has no attribute line, so the class name is what locates it. The key is checked here as well as the value: an unrecognised one is otherwise dropped, and a renamed key silently stops laying out.
    for prop in &class.props {
        let message = match layout_prop_call(&prop.key, &prop.value) {
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
    // A paint-only class never has its layout fn called, so the generated fn can be dead.
    out.push_str("#[allow(dead_code)]\n");
    let _ = writeln!(
        out,
        "fn {}() -> LayoutStyle {{",
        style_function_name(&class.name)
    );
    if let Some(theme) = theme {
        let _ = writeln!(
            out,
            "    #[allow(unused_variables)] let theme = telar::Theme::<{theme}>::default();"
        );
    }
    out.push_str("    LayoutStyle::new()");
    for prop in &class.props {
        if let PropCall::Call(call) = layout_prop_call(&prop.key, &prop.value) {
            let _ = write!(out, "\n        {call}");
        }
    }
    out.push_str("\n}");
    out
}

/// Maps a style property to the `LayoutStyle` builder call it contributes — or reports the value as one this key cannot mean, which is what stops a misspelling from being a property that silently does nothing.
pub fn layout_prop_call(key: &str, value: &str) -> PropCall {
    match layout_call(key, value.trim()) {
        Ok(Some(call)) => PropCall::Call(call),
        Ok(None) => PropCall::Other,
        Err(message) => PropCall::Invalid(message),
    }
}

fn layout_call(key: &str, value: &str) -> Result<Option<String>, String> {
    let call = match key {
        "width" => format!(".width({})", format_number(value)?),
        "height" => format!(".height({})", format_number(value)?),
        "min_width" => format!(".min_width({})", format_number(value)?),
        "min_height" => format!(".min_height({})", format_number(value)?),
        "max_width" => format!(".max_width({})", format_number(value)?),
        "max_height" => format!(".max_height({})", format_number(value)?),
        "basis" | "flex_basis" => format!(".flex_basis({})", format_number(value)?),
        "aspect" | "aspect_ratio" => format!(".aspect_ratio({})", format_number(value)?),
        "wrap" => format!(".{}()", keyword(key, value, registry::WRAP_VALUES)?),
        "self" => format!(".{}()", keyword(key, value, registry::SELF_VALUES)?),
        "padding" | "pad" => format!(".padding_all({})", format_number(value)?),
        "padding_x" | "pad_x" => format!(".padding_horizontal({})", format_number(value)?),
        "padding_y" | "pad_y" => format!(".padding_vertical({})", format_number(value)?),
        // Resolved against the writing direction at layout time, so one build serves LTR and RTL.
        "padding_start" | "pad_start" => {
            format!(".padding_start({})", format_number(value)?)
        }
        "padding_end" | "pad_end" => format!(".padding_end({})", format_number(value)?),
        "margin_start" => format!(".margin_inline_start({})", format_number(value)?),
        "margin_end" => format!(".margin_inline_end({})", format_number(value)?),
        "inset_start" => format!(".inset_start({})", format_number(value)?),
        "inset_end" => format!(".inset_end({})", format_number(value)?),
        "inset_top" => format!(".inset_top({})", format_number(value)?),
        "inset_bottom" => format!(".inset_bottom({})", format_number(value)?),
        // `absolute_fill` is the all-four-at-zero shorthand `overlay` uses; a floating panel wants three edges and its own size on the fourth.
        "absolute" => format!(".{}()", keyword(key, value, registry::ABSOLUTE_VALUES)?),
        // `shown:$open` keeps the subtree it hides — its scroll, its measurements — where an `if` rebuilds it.
        "shown" => match value.is_empty() {
            true => ".shown(true)".to_string(),
            false => format!(".shown({})", crate::view::substitute_reads(value)),
        },
        "gap" => format!(".gap({})", format_number(value)?),
        "gap_x" => format!(".gap_x({})", format_number(value)?),
        "gap_y" => format!(".gap_y({})", format_number(value)?),
        "grow" => format!(".flex_grow({})", format_number(value)?),
        "shrink" => format!(".flex_shrink({})", format_number(value)?),
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

/// One track of a `cols(…)` list. `fr` earns its suffix — it is a real unit with no other spelling — where `px` was the implicit unit everywhere else in the language and a second spelling of a bare number here.
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

/// Whether `value` names a colour this file can resolve, or the message naming what a colour may be.
///
/// A bare name is a `[style]` constant and nothing else. It used to be three namespaces under one spelling, resolved by precedence — a constant, then a token of the theme trait, then *any* remaining name as a field on the theme — so a typo compiled to a field access and failed in rustc against code the author never wrote. `theme.x` is the theme's own spelling and now its only one.
pub fn color(value: &str) -> Result<(), String> {
    let v = value.trim();
    let v = crate::view::redundant_parens(v).unwrap_or(v);
    if v.is_empty() {
        return Err("`color` needs a value: a hex literal, `transparent`, a `$` read, or any Rust expression that yields a `Color`".to_string());
    }
    // The only place a gradient's stops can be checked: inside `linear(…)` there is no attribute to report against.
    if let Some((kind, args)) = crate::gradient::split_call(v) {
        return match crate::gradient::parse(kind, args) {
            Some(gradient) => gradient.stops.iter().try_for_each(|(_, stop)| color(stop)),
            None => Err(format!(
                "`{v}` is not a gradient: write `{kind}(…)` with two or more stops, each a colour, optionally followed by where it sits"
            )),
        };
    }
    Ok(())
}

/// Resolves a numeric value to the Rust expression it stands for.
///
/// Two token shapes resolve here — `50%` and a `$` read — and a plain literal is normalised so `12` reaches an `f32` parameter. Everything else is the author's own Rust, spliced as written for rustc to judge against this attribute's own line.
pub fn format_number(value: &str) -> Result<String, String> {
    // Emitted, the markup's delimiting parens warn `unused_parens` in code the author cannot edit.
    let v = value.trim();
    let v = crate::view::redundant_parens(v).unwrap_or(v);
    // A token shape rather than a key's private grammar, so it expands wherever written and rustc rejects it under a key that is not a length.
    if let Some(pct) = v.strip_suffix('%') {
        return match pct.trim().parse::<f32>() {
            Ok(n) => Ok(format!("SizeDimension::Percent({})", format_f32(n / 100.0))),
            Err(_) => Err(format!("`{v}` is not a percentage")),
        };
    }
    // Emitted inline, which is correct in both places the style expression lands: at construction, and again inside the `styled_by` effect a reactive layout prop grows.
    if v.contains('$') {
        return Ok(crate::view::substitute_reads(v));
    }
    if let Ok(n) = v.parse::<f32>() {
        return Ok(format_f32(n));
    }
    Ok(v.to_string())
}

/// The integer twin of [`format_number`], for the one property that counts rather than measures: `lines:2` feeds a `u16`, so it stays `2` where a length would become `2.0`.
pub fn format_integer(value: &str) -> String {
    let v = value.trim();
    let v = crate::view::redundant_parens(v).unwrap_or(v);
    match v.contains('$') {
        true => crate::view::substitute_reads(v),
        false => v.to_string(),
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

/// Builds a `Color::rgba(...)` const expression from a hex string, at any length [`telar_parser::parse_hex`] accepts. Anything else falls back to opaque black, which the parser has already rejected before a value reaches here.
pub fn hex_to_color_expr(hex: &str) -> String {
    let [r, g, b, a] = telar_parser::parse_hex(hex).unwrap_or([0, 0, 0, 255]);
    format!("Color::rgba({r}.0 / 255.0, {g}.0 / 255.0, {b}.0 / 255.0, {a}.0 / 255.0)")
}

#[cfg(test)]
mod tests {
    use super::*;
    use telar_parser::StyleProp;

    fn call(key: &str, value: &str) -> Option<String> {
        match layout_prop_call(key, value) {
            PropCall::Call(call) => Some(call),
            _ => None,
        }
    }

    fn invalid(key: &str, value: &str) -> Option<String> {
        match layout_prop_call(key, value) {
            PropCall::Invalid(message) => Some(message),
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

    /// The `theme` binding the view makes is read like any other reactive handle, in any numeric prop.
    #[test]
    fn a_theme_read_resolves_in_any_numeric_prop() {
        for (key, expected) in [
            ("pad", ".padding_all(theme.get().gutter)"),
            ("gap", ".gap(theme.get().gutter)"),
            ("width", ".width(theme.get().gutter)"),
            ("margin_start", ".margin_inline_start(theme.get().gutter)"),
        ] {
            assert_eq!(
                call(key, "$theme.gutter").as_deref(),
                Some(expected),
                "{key}"
            );
        }
    }

    /// A bare name is the author's own, whatever the theme happens to call a field of its own.
    #[test]
    fn a_bare_ident_is_the_name_the_author_wrote() {
        assert_eq!(
            call("pad", "card_gap").as_deref(),
            Some(".padding_all(card_gap)")
        );
    }

    #[test]
    fn radius_is_ignored() {
        assert!(matches!(layout_prop_call("radius", "6"), PropCall::Other));
    }

    #[test]
    fn aspect_maps_to_aspect_ratio() {
        assert_eq!(call("aspect", "1").as_deref(), Some(".aspect_ratio(1.0)"));
        assert_eq!(
            call("aspect_ratio", "1.5").as_deref(),
            Some(".aspect_ratio(1.5)")
        );
    }

    /// A value the transpiler cannot resolve is the author's own Rust and reaches rustc, which names it against this `.rsx` line through the source map. There used to be a rejection here for anything that was not name-shaped, which is a judgement about Rust made by something that does not parse Rust.
    #[test]
    fn a_name_the_author_has_in_scope_is_carried_through() {
        for value in ["side", "props.pad", "crate::scale::md()", "TRACK_H"] {
            assert!(
                call("gap", value).is_some(),
                "`gap:{value}` names something the author has in scope"
            );
        }
    }

    /// A percentage resolves against the containing block wherever CSS says it does — every length, not the six size keys that happened to call the parser that knew about `%`.
    #[test]
    fn a_percentage_is_a_length_wherever_a_length_is() {
        for key in [
            "width",
            "pad",
            "padding_x",
            "gap",
            "margin_start",
            "inset_top",
        ] {
            assert!(
                call(key, "50%")
                    .as_deref()
                    .is_some_and(|c| c.contains("SizeDimension::Percent(0.5)")),
                "`{key}:50%` should resolve"
            );
        }
        for key in ["grow", "aspect"] {
            assert!(
                call(key, "50%")
                    .as_deref()
                    .is_some_and(|c| c.contains("SizeDimension::Percent(0.5)")),
                "`{key}:50%` expands and is rustc's to reject"
            );
        }
    }

    /// S3: a value outside a closed keyword set now says what the set is, on the attribute, instead of the property being dropped and the layout coming out subtly wrong.
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

    /// A class property is written where no element is, so it has no attribute line — but it is the same misspelling, and it used to be dropped just as silently.
    #[test]
    fn a_class_property_reports_its_own_bad_value() {
        let section = StyleSection {
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

    /// A class property nobody recognises used to be dropped on the floor, which is how a renamed key goes on compiling and quietly stops laying the class out. Element attributes have been checked this way since keys were checked at all; classes never were.
    #[test]
    fn a_class_property_with_an_unknown_key_is_rejected() {
        let section = StyleSection {
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

    /// A paint key is not a layout property and must not be mistaken for an unknown one: it reaches the `RectStyle` by another path entirely.
    #[test]
    fn a_paint_property_in_a_class_is_not_mistaken_for_an_unknown_key() {
        let section = StyleSection {
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
