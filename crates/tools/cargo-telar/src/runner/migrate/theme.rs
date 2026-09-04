//! Theme rewrites: the sigil a view read gains, the call a logic block keeps, and the style constants that become logic.

use super::text::{replace_outside_strings, replace_whole_name, starts_a_name};
use super::zones::{Section, zones};
/// `theme.primary` → `$theme.primary`. The view binds `theme` as a handle, so a theme read is the same `$` that reads a signal — and it re-reads where it is written instead of freezing at construction.
pub(super) fn theme_reads(body: &str) -> String {
    replace_outside_strings(body, |chunk| replace_theme_name(chunk, "$theme"))
}

/// A bare `theme()` → `theme.get()` in `[logic]`, which sits below the binding and so no longer sees the crate's own accessor function. A qualified `crate::core::theme::theme()` still names that function and is left alone — which is also what a nested `fn` inside `[logic]` needs, since it cannot see the binding.
pub(super) fn theme_calls(body: &str) -> String {
    replace_outside_strings(body, |chunk| {
        let bytes = chunk.as_bytes();
        let (mut out, mut i) = (String::with_capacity(chunk.len()), 0usize);
        while i < chunk.len() {
            if chunk[i..].starts_with("theme()") && starts_a_name(bytes, i) {
                out.push_str("theme.get()");
                i += "theme()".len();
                continue;
            }
            let ch = chunk[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
        out
    })
}

pub(super) fn replace_theme_name(chunk: &str, to: &str) -> String {
    let bytes = chunk.as_bytes();
    let (mut out, mut i) = (String::with_capacity(chunk.len()), 0usize);
    while i < chunk.len() {
        if chunk[i..].starts_with("theme")
            && starts_a_name(bytes, i)
            && bytes.get(5 + i) == Some(&b'.')
        {
            out.push_str(to);
            i += "theme".len();
            continue;
        }
        let ch = chunk[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Moves every `[style]` constant into `[logic]` as a `const`, and rewrites the names that referred to it. `[style]` keeps classes — named bundles of properties, which are reuse rather than a second evaluation model.
pub(super) fn style_constants_to_logic(source: &str) -> String {
    let constants: Vec<(String, String, String)> = zones(source)
        .iter()
        .filter(|z| z.section == Section::Style)
        .flat_map(|z| z.body.lines())
        .filter_map(style_constant)
        .collect();
    if constants.is_empty() {
        return source.to_string();
    }

    let mut out = String::with_capacity(source.len());
    for zone in zones(source) {
        out.push_str(zone.header);
        match zone.section {
            Section::Logic => {
                out.push_str(zone.body.trim_end_matches('\n'));
                out.push_str("\n\n");
                for (name, ty, value) in &constants {
                    out.push_str(&format!("const {}: {ty} = {value};\n", name.to_uppercase()));
                }
                out.push('\n');
            }
            Section::Style => {
                for line in zone.body.split_inclusive('\n') {
                    if style_constant(line).is_none() {
                        out.push_str(line);
                    }
                }
            }
            _ => out.push_str(&replace_outside_strings(zone.body, |chunk| {
                let mut chunk = chunk.to_string();
                for (name, _, _) in &constants {
                    chunk = replace_whole_name(&chunk, name, &name.to_uppercase());
                }
                chunk
            })),
        }
    }
    out
}

/// `(name, Rust type, Rust value)` for a `[style]` constant line, or `None` for a class header, a class property (indented) or a blank.
pub(super) fn style_constant(line: &str) -> Option<(String, String, String)> {
    if line.starts_with([' ', '\t']) || line.trim().is_empty() || line.trim_start().starts_with('@')
    {
        return None;
    }
    let (name, value) = line.split_once(':')?;
    let (name, value) = (name.trim(), value.trim());
    if name.is_empty() || value.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_')
    {
        return None;
    }
    let (ty, rust) = match value.strip_prefix('#') {
        Some(hex) => ("Color".to_string(), hex_to_color(hex)?),
        None => match value.parse::<f32>() {
            Ok(n) => ("f32".to_string(), format!("{n:?}")),
            Err(_) => ("&str".to_string(), format!("{value:?}")),
        },
    };
    Some((name.to_string(), ty, rust))
}

pub(super) fn hex_to_color(hex: &str) -> Option<String> {
    let expand = |c: char| u8::from_str_radix(&format!("{c}{c}"), 16).ok();
    let byte = |s: &str| u8::from_str_radix(s, 16).ok();
    let chars: Vec<char> = hex.chars().collect();
    let [r, g, b, a] = match chars.len() {
        3 => [expand(chars[0])?, expand(chars[1])?, expand(chars[2])?, 255],
        4 => [
            expand(chars[0])?,
            expand(chars[1])?,
            expand(chars[2])?,
            expand(chars[3])?,
        ],
        6 => [byte(&hex[0..2])?, byte(&hex[2..4])?, byte(&hex[4..6])?, 255],
        8 => [
            byte(&hex[0..2])?,
            byte(&hex[2..4])?,
            byte(&hex[4..6])?,
            byte(&hex[6..8])?,
        ],
        _ => return None,
    };
    let f = |c: u8| format!("{:.3}", c as f32 / 255.0);
    Some(format!(
        "Color::rgba({}, {}, {}, {})",
        f(r),
        f(g),
        f(b),
        f(a)
    ))
}
