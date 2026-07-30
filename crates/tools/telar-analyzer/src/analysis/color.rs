//! `textDocument/documentColor` + `colorPresentation`: inline swatches and a color picker for the `.rsx` styling DSL.
//!
//! v1 scope is **color literals** — hex (`#rgb`/`#rrggbb`/`#rrggbbaa`) and the three keyword colors (`white`/`black`/`transparent`) — at the two places they appear with position info: `[style]` constant values and `[view]` attribute values. These are unambiguously editable, so the picker can write a hex back without clobbering a theme binding. Theme-token references (`color:primary` where `primary` is a theme field) are intentionally skipped: resolving them to RGBA would require evaluating the theme's Rust, and rewriting them to a hex would drop the reactive binding.

use lsp_types::{Color, ColorInformation, ColorPresentation, Range};
use telar_parser::{RsxDocument, ViewNode};
use telar_transpiler::color_attr_keys;

use crate::position::{Section, find_section_at};
use crate::text::offset_to_position;

/// Collects every editable color literal in the document, with its source range and resolved RGBA.
pub fn document_colors(doc: &RsxDocument, source: &str) -> Vec<ColorInformation> {
    let mut out = Vec::new();

    // `[style]`: both constants (`primary: #hex`) and class paint props (`fill: #hex`). Scanned from the source line-by-line because the AST carries no per-prop position.
    collect_style_colors(source, &mut out);

    // `[view]` (and `[preview]`) attribute literals.
    collect_view_colors(&doc.view.nodes, source, &mut out);
    for preview in &doc.previews {
        collect_view_colors(&preview.body, source, &mut out);
    }

    out
}

/// Scans the `[style]` section for `key: <color>` lines (constant *or* class prop) and emits a swatch on the value when it is a color literal.
fn collect_style_colors(source: &str, out: &mut Vec<ColorInformation>) {
    for (line_idx, line_text) in source.lines().enumerate() {
        if find_section_at(source, line_idx as u32) != Section::Style {
            continue;
        }
        let Some(colon) = line_text.find(':') else {
            continue;
        };
        // An inline class header (`@badge: padding_x:6 …`) has a `:` but its value is props, not color.
        if line_text[..colon].trim_start().starts_with('@') {
            continue;
        }
        let after = &line_text[colon + 1..];
        let value = after.trim();
        let color = if value.starts_with('#') {
            parse_hex(value)
        } else {
            keyword_color(value)
        };
        if let Some(color) = color {
            let value_col = colon + 1 + (after.len() - after.trim_start().len());
            if let Some(line_start) = line_start_byte(source, line_idx)
                && let Some(range) = byte_range(source, line_start + value_col, value.len())
            {
                out.push(ColorInformation { range, color });
            }
        }
    }
}

/// The picker write-back: the chosen RGBA as a hex string, applied over the swatch's range.
pub fn color_presentations(color: Color) -> Vec<ColorPresentation> {
    vec![ColorPresentation {
        label: hex_string(color),
        // No `text_edit`: the client applies `label` over the requested color range.
        text_edit: None,
        additional_text_edits: None,
    }]
}

fn collect_view_colors(nodes: &[ViewNode], source: &str, out: &mut Vec<ColorInformation>) {
    for node in nodes {
        match node {
            ViewNode::Element(el) => {
                for attr in &el.attributes {
                    // A quoted value is a string literal, never a color.
                    if attr.is_quoted {
                        continue;
                    }
                    if let Some(color) = literal_color(&attr.key, &attr.value)
                        && let Some(range) = byte_range(source, attr.value_start, attr.value.len())
                    {
                        out.push(ColorInformation { range, color });
                    }
                }
                collect_view_colors(&el.children, source, out);
            }
            ViewNode::IfBlock(block) => {
                collect_view_colors(&block.then_branch, source, out);
                if let Some(else_branch) = &block.else_branch {
                    collect_view_colors(else_branch, source, out);
                }
            }
            ViewNode::ForBlock(block) => collect_view_colors(&block.body, source, out),
            ViewNode::LetStmt(_) => {}
        }
    }
}

/// Resolves an attribute value to a color *literal* (hex anywhere; keyword only under a color key).
fn literal_color(key: &str, value: &str) -> Option<Color> {
    let v = value.trim();
    if v.starts_with('#') {
        return parse_hex(v);
    }
    if color_attr_keys().contains(&key) {
        return keyword_color(v);
    }
    None
}

/// The keyword colors the transpiler recognizes (`color_expr`); other names are theme tokens.
fn keyword_color(value: &str) -> Option<Color> {
    let [r, g, b, a] = telar_transpiler::keyword_color_rgba(value)?;
    Some(rgba(r, g, b, a))
}

/// Parses `#rgb` / `#rrggbb` / `#rrggbbaa` (matching the transpiler's `hex_to_color_expr`).
pub(crate) fn parse_hex(hex: &str) -> Option<Color> {
    let h = hex.strip_prefix('#')?;
    if !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |s: &str| u8::from_str_radix(s, 16).ok();
    let (r, g, b, a) = match h.len() {
        3 => {
            let dup = |c: &str| byte(&format!("{c}{c}"));
            (dup(&h[0..1])?, dup(&h[1..2])?, dup(&h[2..3])?, 255)
        }
        6 => (byte(&h[0..2])?, byte(&h[2..4])?, byte(&h[4..6])?, 255),
        8 => (
            byte(&h[0..2])?,
            byte(&h[2..4])?,
            byte(&h[4..6])?,
            byte(&h[6..8])?,
        ),
        _ => return None,
    };
    Some(rgba(r, g, b, a))
}

pub(crate) fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color {
        red: r as f32 / 255.0,
        green: g as f32 / 255.0,
        blue: b as f32 / 255.0,
        alpha: a as f32 / 255.0,
    }
}

/// Formats an LSP `Color` as `#rrggbb` (or `#rrggbbaa` when not fully opaque).
pub(crate) fn hex_string(color: Color) -> String {
    let to_u8 = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    let (r, g, b, a) = (
        to_u8(color.red),
        to_u8(color.green),
        to_u8(color.blue),
        to_u8(color.alpha),
    );
    if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    }
}

/// The LSP range for an `(offset, len)` byte span in `source`, with UTF-16 columns per LSP.
fn byte_range(source: &str, start: usize, len: usize) -> Option<Range> {
    if start + len > source.len() {
        return None;
    }
    Some(Range {
        start: offset_to_position(source, start),
        end: offset_to_position(source, start + len),
    })
}

/// Byte offset where 0-based `line0` begins.
fn line_start_byte(source: &str, line0: usize) -> Option<usize> {
    if line0 == 0 {
        return Some(0);
    }
    let mut seen = 0usize;
    for (i, ch) in source.char_indices() {
        if ch == '\n' {
            seen += 1;
            if seen == line0 {
                return Some(i + 1);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use telar_parser::parse;

    fn color_at<'a>(infos: &'a [ColorInformation], src: &str, frag: &str) -> Option<&'a Color> {
        let byte = src.find(frag)?;
        let pos = offset_to_position(src, byte);
        infos
            .iter()
            .find(|i| i.range.start == pos)
            .map(|i| &i.color)
    }

    #[test]
    fn style_constant_hex_gets_a_swatch() {
        let src = "[style]\nprimary: #4361ee\n[view]\ncol\n";
        let doc = parse(src).unwrap();
        let infos = document_colors(&doc, src);
        let color = color_at(&infos, src, "#4361ee").expect("swatch on the hex constant");
        assert!((color.red - 67.0 / 255.0).abs() < 1e-6);
        assert!((color.green - 97.0 / 255.0).abs() < 1e-6);
        assert!((color.blue - 238.0 / 255.0).abs() < 1e-6);
        assert_eq!(color.alpha, 1.0);
    }

    #[test]
    fn view_hex_and_keyword_attrs_get_swatches_but_quoted_and_tokens_do_not() {
        let src =
            "[view]\nbox fill:#ff0000 stroke:white color:primary\n    text label:\"#nothex\"\n";
        let doc = parse(src).unwrap();
        let infos = document_colors(&doc, src);
        // Inline hex.
        assert!(
            color_at(&infos, src, "#ff0000").is_some(),
            "hex attr swatch"
        );
        // Keyword under a color key.
        assert!(color_at(&infos, src, "white").is_some(), "keyword swatch");
        // A theme-token reference is skipped (no RGBA, would clobber the binding).
        assert!(
            color_at(&infos, src, "primary").is_none(),
            "token ref must not get a swatch"
        );
        // A quoted value is a string, not a color.
        assert!(
            color_at(&infos, src, "#nothex").is_none(),
            "quoted value must not get a swatch"
        );
    }

    #[test]
    fn signal_color_reference_degrades_without_panic() {
        // `fill:$accent` (transpiler T-3.x) is a reactive read, not a literal; it must be silently skipped (no swatch), same as a theme token, rather than panicking on the `$` sigil.
        let src = "[view]\nbox fill:$accent stroke:$accent\n";
        let doc = parse(src).unwrap();
        let infos = document_colors(&doc, src);
        assert!(
            color_at(&infos, src, "$accent").is_none(),
            "a signal reference must not get a swatch"
        );
    }

    #[test]
    fn presentation_round_trips_to_hex() {
        let opaque = color_presentations(rgba(67, 97, 238, 255));
        assert_eq!(opaque[0].label, "#4361ee");
        let translucent = color_presentations(rgba(0, 0, 0, 128));
        assert_eq!(translucent[0].label, "#00000080");
    }

    #[test]
    fn class_prop_hex_gets_a_swatch() {
        // A paint prop inside a `@class` block (not a top-level constant) must still get a swatch.
        let src = "[style]\n@card\n    fill: #ff8800\n    radius: 8\n[view]\ncol @card\n";
        let doc = parse(src).unwrap();
        let infos = document_colors(&doc, src);
        assert!(
            color_at(&infos, src, "#ff8800").is_some(),
            "class-prop fill should get a swatch"
        );
        // Only the fill is a color — `radius: 8` and `direction` must not add swatches.
        assert_eq!(
            infos.len(),
            1,
            "non-color props must not get swatches: {infos:?}"
        );
    }

    #[test]
    fn short_hex_expands() {
        let src = "[style]\naccent: #f0a\n[view]\ncol\n";
        let doc = parse(src).unwrap();
        let infos = document_colors(&doc, src);
        let color = color_at(&infos, src, "#f0a").expect("short hex swatch");
        assert!((color.red - 1.0).abs() < 1e-6);
        assert_eq!(color.green, 0.0);
        assert!((color.blue - 170.0 / 255.0).abs() < 1e-6);
    }
}
