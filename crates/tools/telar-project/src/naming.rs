//! Shared identifier conversions between RSX names and generated Rust names.

/// Returns true if `c` is a word-separator: `.`, `-`, `_`, or whitespace.
fn is_separator(c: char) -> bool {
    matches!(c, '.' | '-' | '_' | ' ' | '\t')
}

/// Converts an RSX name (`card-title`, `btn.primary`) into a snake_case identifier. Separators (`.`, `-`, `_`, whitespace) become `_`. Leading digits are prefixed with `_` to produce a valid Rust identifier.
pub fn to_snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 1);
    let mut prev_was_sep = false;
    for (i, c) in name.chars().enumerate() {
        if is_separator(c) {
            if !out.is_empty() {
                prev_was_sep = true;
            }
        } else if c.is_ascii_alphanumeric() {
            if i == 0 && c.is_ascii_digit() {
                out.push('_');
            }
            if prev_was_sep {
                out.push('_');
                prev_was_sep = false;
            }
            out.push(c.to_ascii_lowercase());
        }
    }
    out
}

/// Converts an RSX name (`shape_card`, `info.card`) into PascalCase (`ShapeCard`, `InfoCard`). Separators (`.`, `-`, `_`, whitespace) trigger capitalization of the next word. Non-alphanumeric, non-separator chars are stripped. A leading digit is prefixed with `_`.
pub fn to_pascal_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut next_upper = true;
    let mut first_char = true;
    for c in name.chars() {
        if is_separator(c) {
            next_upper = true;
        } else if c.is_ascii_alphanumeric() {
            if first_char && c.is_ascii_digit() {
                out.push('_');
            }
            first_char = false;
            if next_upper && !c.is_ascii_digit() {
                out.extend(c.to_uppercase());
            } else {
                out.push(c);
            }
            next_upper = false;
        }
    }
    out
}

/// Generated `LayoutStyle` constructor name for a style class: `card` -> `style_card`.
pub fn style_function_name(class: &str) -> String {
    format!("style_{}", to_snake_case(class))
}

/// Generated color/number constant name: `card-border` -> `COLOR_CARD_BORDER`.
pub fn constant_name(prefix: &str, name: &str) -> String {
    format!("{prefix}{}", to_snake_case(name).to_ascii_uppercase())
}

/// Generated preview entries const name for a file stem: `card` -> `CARD_PREVIEW_ENTRIES`. This must match the name emitted by the transpiler in the generated `.rs` file.
pub fn preview_entries_const_name(stem: &str) -> String {
    format!(
        "{}_PREVIEW_ENTRIES",
        to_snake_case(stem).to_ascii_uppercase()
    )
}

/// Whether `b` may appear inside a Rust identifier.
pub fn is_ident_byte(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphanumeric()
}

/// Whether `s` is a valid Rust identifier: starts with `_` or a letter, rest `_`/alphanumeric.
pub fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c == '_' || c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

#[cfg(test)]
#[path = "naming_test.rs"]
mod tests;
