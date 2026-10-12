//! Holds the catalogue to its rule: every length, corner, border, colour and weight a component draws comes from a theme role, the control size, or the component's own `XTokens` — through `shared`'s helpers — and never from a number written in the component.
//!
//! The scan reads every non-test source file of the crate (`previews/` is demo content and is left out) and fails on anything that breaks the rule, naming the file and the kind. There is no allowance: a value no role or token can say belongs in a new role or in the component's tokens, not in an exception.
//!
//! The kinds:
//!
//! - `radius-literal`: `BorderRadius::all(4.0)`. Read a role: `radius_control`, `radius_surface`, `radius_dialog`, `radius_item`.
//! - `pill-by-hand`: `BorderRadius::all(size / 2.0)`. Use `pill(rect)` or `pill_radius(side)`.
//! - `radius-factor`: `radius() * 3.0`, `radius_md() * 2.0`. A primitive scaled by a factor of the component's own is a role nobody named; read the role.
//! - `colour-literal`: `Color::rgb(…)`, `Color::rgba(…)`, `Color::from_hex(…)` and the other constructors, written with numbers or a string. Read a theme colour (`inverse_surface`, `overlay`, `thumb`, …). A constructor fed values the code worked out — a colour-model conversion, a hex the user typed — draws no colour of its own, and is not one.
//! - `fixed-paint`: `with_fill(Color::WHITE)`, `Border::uniform(Color::BLACK, …)`: a named constant painted as a surface or a frame. `Color::TRANSPARENT` is no paint, and is allowed.
//! - `spacing-literal`: `padding_*(10.0)`, `gap(8.0)`, `margin_*(4.0)`. Use `gap_*`, `control_pad_x`, `panel_pad`, `spacing`.
//! - `size-literal`: `width(180.0)`, `height(32.0)`, `min_*`/`max_*` the same. Use `control_height`, `row_height`, `icon_size`, or a token.
//! - `border-literal`: `Border::uniform(c, 1.5)`, `Border::per_side(c, 0.0, 0.0, 1.0, 0.0)`. Use `border_width`, `border_width_strong`, `focus_ring`.
//! - `weight-literal`: `with_font_weight(600)`. Use `strong_weight`.
//!
//! Zero is not a literal here: `gap(0.0)` and `BorderRadius::all(0.0)` say "none", which no theme changes.

use std::collections::BTreeMap;
use std::path::PathBuf;

const PADDING: &[&str] = &[
    "padding_all",
    "padding_horizontal",
    "padding_vertical",
    "padding_top",
    "padding_bottom",
    "padding_left",
    "padding_right",
    "padding_start",
    "padding_end",
    "margin_all",
    "margin_horizontal",
    "margin_vertical",
    "margin_top",
    "margin_bottom",
    "margin_left",
    "margin_right",
    "gap",
    "row_gap",
    "column_gap",
];

const SIZES: &[&str] = &[
    "width",
    "height",
    "min_width",
    "min_height",
    "max_width",
    "max_height",
];

const COLOUR_CONSTRUCTORS: &[&str] = &[
    "Color::rgb(",
    "Color::rgba(",
    "Color::from_rgb_u8(",
    "Color::from_rgba_u8(",
    "Color::from_hex(",
    "Color::from_oklch(",
    "Color::from_oklcha(",
];

const PRIMITIVE_RADII: &[&str] = &["radius()", "radius_sm()", "radius_md()", "radius_lg()"];

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The sources the rule covers, relative to `src/`.
fn sources() -> Vec<String> {
    let src = crate_root().join("src");
    let mut files: Vec<String> = std::fs::read_dir(&src)
        .expect("src/ is readable")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .filter_map(|path| Some(path.file_name()?.to_str()?.to_string()))
        .filter(|name| !name.ends_with("_test.rs") && name != "test_support.rs")
        .collect();
    files.sort();
    files
}

/// `source` with comments and the contents of string and character literals blanked out, so a doc comment quoting `gap(8.0)`, or a label that reads like code, is not an offence.
fn without_comments(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '/' if chars.get(i + 1) == Some(&'/') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '"' => {
                out.push('"');
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    if chars[i] == '\\' {
                        i += 1;
                    }
                    i += 1;
                }
                out.push('"');
                i += 1;
            }
            '\'' if chars.get(i + 2) == Some(&'\'') => {
                out.push_str("' '");
                i += 3;
            }
            '\'' if chars.get(i + 1) == Some(&'\\') => {
                out.push_str("' '");
                i += 2;
                while i < chars.len() && chars[i] != '\'' {
                    i += 1;
                }
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// The top-level arguments of the call whose `(` is at `open`.
fn arguments(text: &str, open: usize) -> Vec<String> {
    let mut depth = 0usize;
    let mut args = Vec::new();
    let mut current = String::new();
    for c in text[open..].chars() {
        match c {
            '(' | '[' | '{' => {
                depth += 1;
                if depth > 1 {
                    current.push(c);
                }
            }
            ')' | ']' | '}' => {
                depth -= 1;
                if depth == 0 {
                    args.push(current.trim().to_string());
                    return args;
                }
                current.push(c);
            }
            ',' if depth == 1 => args.push(std::mem::take(&mut current).trim().to_string()),
            _ => current.push(c),
        }
    }
    args
}

/// A number written out.
fn number(arg: &str) -> bool {
    let arg = arg.trim_end_matches("_f32").trim_end_matches("f32");
    arg.parse::<f32>().is_ok()
}

/// A number written out, other than zero.
fn nonzero_literal(arg: &str) -> bool {
    let arg = arg.trim_end_matches("_f32").trim_end_matches("f32");
    arg.parse::<f32>().is_ok_and(|n| n != 0.0)
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Every call to the method or path `name` in `text`, as the offset of its `(`. A method name only counts as itself, not as the tail of a longer one.
fn calls<'a>(text: &'a str, name: &'a str) -> impl Iterator<Item = usize> + 'a {
    let needle = format!("{name}(");
    text.match_indices(&needle)
        .map(|(at, _)| at)
        .filter(move |&at| at == 0 || !is_ident_byte(text.as_bytes()[at - 1]))
        .map(move |at| at + name.len())
        .collect::<Vec<_>>()
        .into_iter()
}

fn count(text: &str) -> BTreeMap<&'static str, usize> {
    let mut found: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut add = |kind: &'static str| *found.entry(kind).or_default() += 1;

    for open in calls(text, "BorderRadius::all") {
        let args = arguments(text, open);
        let arg = args.first().map(String::as_str).unwrap_or_default();
        if nonzero_literal(arg) {
            add("radius-literal");
        } else if arg.replace(' ', "").ends_with("/2.0") {
            add("pill-by-hand");
        }
    }
    for radius in PRIMITIVE_RADII {
        for (at, _) in text.match_indices(radius) {
            let prefixed = at > 0 && is_ident_byte(text.as_bytes()[at - 1]);
            let next = text[at + radius.len()..].trim_start();
            if !prefixed && (next.starts_with('*') || next.starts_with('/')) {
                add("radius-factor");
            }
        }
    }
    for constructor in COLOUR_CONSTRUCTORS {
        for (at, _) in text.match_indices(constructor) {
            let written = arguments(text, at + constructor.len() - 1)
                .iter()
                .all(|arg| arg.starts_with('"') || number(arg));
            if written {
                add("colour-literal");
            }
        }
    }
    for painter in ["with_fill", "Border::uniform", "Border::per_side"] {
        for open in calls(text, painter) {
            let args = arguments(text, open);
            let paint = args.first().map(String::as_str).unwrap_or_default();
            let named = paint
                .strip_prefix("Color::")
                .is_some_and(|name| name.chars().all(|c| c.is_ascii_uppercase() || c == '_'));
            if named && paint != "Color::TRANSPARENT" {
                add("fixed-paint");
            }
            if painter != "with_fill" && args.iter().skip(1).any(|w| nonzero_literal(w)) {
                add("border-literal");
            }
        }
    }
    for method in PADDING {
        for open in calls(text, method) {
            if arguments(text, open).iter().any(|a| nonzero_literal(a)) {
                add("spacing-literal");
            }
        }
    }
    for method in SIZES {
        for open in calls(text, method) {
            if arguments(text, open).iter().any(|a| nonzero_literal(a)) {
                add("size-literal");
            }
        }
    }
    for open in calls(text, "with_font_weight") {
        if arguments(text, open).iter().any(|a| nonzero_literal(a)) {
            add("weight-literal");
        }
    }
    found
}

/// What the sources hold now, by file and kind.
fn scanned() -> BTreeMap<(String, String), usize> {
    let src = crate_root().join("src");
    let mut all = BTreeMap::new();
    for file in sources() {
        let text = std::fs::read_to_string(src.join(&file)).expect("a source is readable");
        for (kind, n) in count(&without_comments(&text)) {
            all.insert((file.clone(), kind.to_string()), n);
        }
    }
    all
}

#[test]
fn components_draw_no_literal() {
    let problems: Vec<String> = scanned()
        .into_iter()
        .map(|((file, kind), n)| {
            format!("{file}: {n} `{kind}` — read a theme role, a `shared` helper or the component's tokens instead")
        })
        .collect();
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn the_scan_sees_each_kind_and_skips_comments_and_zero() {
    let source = r#"
        // gap(8.0) in a comment
        let a = BorderRadius::all(4.0);
        let b = BorderRadius::all(size / 2.0);
        let c = BorderRadius::all(0.0);
        let d = shared::radius() * 3.0;
        let e = shared::pill_radius(side) * 1.0;
        let f = Color::rgba(0.0, 0.0, 0.0, 0.5);
        let g = rect.with_fill(Color::WHITE);
        let h = rect.with_fill(Color::TRANSPARENT);
        let i = Border::uniform(shared::accent(), 2.0);
        let j = LayoutStyle::new().padding_horizontal(10.0).gap(0.0).height(32.0);
        let k = text.with_font_weight(600);
        let l = "gap(8.0)";
        let m = '"';
        let n = gap(8.0);
        let o = Color::rgba(r + m, g + m, b + m, alpha);
        let p = Color::from_hex(typed.trim());
        let q = Color::from_hex("ffffff");
    "#;
    let found = count(&without_comments(source));
    let expected: BTreeMap<&str, usize> = [
        ("radius-literal", 1),
        ("pill-by-hand", 1),
        ("radius-factor", 1),
        ("colour-literal", 2),
        ("fixed-paint", 1),
        ("border-literal", 1),
        ("spacing-literal", 2),
        ("size-literal", 1),
        ("weight-literal", 1),
    ]
    .into_iter()
    .collect();
    assert_eq!(found, expected);
}
