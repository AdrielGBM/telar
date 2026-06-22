//! Shared identifier conversions between RSX names and generated Rust names.

/// Returns true if `c` is a word-separator: `.`, `-`, `_`, or whitespace.
fn is_separator(c: char) -> bool {
    matches!(c, '.' | '-' | '_' | ' ' | '\t')
}

/// Converts an RSX name (`card-title`, `btn.primary`) into a snake_case identifier.
/// Separators (`.`, `-`, `_`, whitespace) become `_`. Leading digits are prefixed with `_`
/// to produce a valid Rust identifier.
pub fn to_snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 1);
    let mut prev_was_sep = false;
    for (i, c) in name.chars().enumerate() {
        if is_separator(c) {
            // collapse consecutive separators into one `_`, but only if we have output so far
            if !out.is_empty() {
                prev_was_sep = true;
            }
        } else if c.is_ascii_alphanumeric() {
            // prefix a leading digit with `_` to keep identifiers valid
            if i == 0 && c.is_ascii_digit() {
                out.push('_');
            }
            if prev_was_sep {
                out.push('_');
                prev_was_sep = false;
            }
            out.push(c.to_ascii_lowercase());
        }
        // non-separator, non-alphanumeric chars are silently dropped
    }
    out
}

/// Converts an RSX name (`shape_card`, `info.card`) into PascalCase (`ShapeCard`, `InfoCard`).
/// Separators (`.`, `-`, `_`, whitespace) trigger capitalization of the next word.
/// Non-alphanumeric, non-separator chars are stripped. A leading digit is prefixed with `_`.
pub fn to_pascal_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut next_upper = true;
    let mut first_char = true;
    for c in name.chars() {
        if is_separator(c) {
            next_upper = true;
        } else if c.is_ascii_alphanumeric() {
            // prefix a leading digit with `_`; the digit itself is not capitalized
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
        // non-separator, non-alphanumeric chars are silently dropped
    }
    out
}

/// Generated `LayoutStyle` constructor name for a style class: `card` -> `style_card`.
pub fn style_fn_name(class: &str) -> String {
    format!("style_{}", to_snake_case(class))
}

/// Generated color/number constant name: `card-border` -> `COLOR_CARD_BORDER`.
pub fn const_name(prefix: &str, name: &str) -> String {
    format!("{prefix}{}", to_snake_case(name).to_ascii_uppercase())
}

/// Generated preview entries const name for a file stem: `card` -> `CARD_PREVIEW_ENTRIES`.
/// This must match the name emitted by the transpiler in the generated `.rs` file.
pub fn preview_entries_const_name(stem: &str) -> String {
    format!(
        "{}_PREVIEW_ENTRIES",
        to_snake_case(stem).to_ascii_uppercase()
    )
}

pub(crate) fn is_ident_byte(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphanumeric()
}

pub(crate) fn mentions_ident(code: &str, ident: &str) -> bool {
    let bytes = code.as_bytes();
    let mut start = 0;
    while let Some(pos) = code[start..].find(ident) {
        let abs = start + pos;
        let before_ok = abs == 0 || !is_ident_byte(bytes[abs - 1]);
        let after = abs + ident.len();
        let after_ok = after >= bytes.len() || !is_ident_byte(bytes[after]);
        if before_ok && after_ok {
            return true;
        }
        start = abs + ident.len();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- to_snake_case ---

    #[test]
    fn snake_basic_already_snake() {
        assert_eq!(to_snake_case("btn_primary"), "btn_primary");
        assert_eq!(to_snake_case("hello_world"), "hello_world");
    }

    #[test]
    fn snake_hyphen_separator() {
        assert_eq!(to_snake_case("my-component"), "my_component");
        assert_eq!(to_snake_case("card-title"), "card_title");
    }

    #[test]
    fn snake_dot_separator() {
        assert_eq!(to_snake_case("btn.primary"), "btn_primary");
        assert_eq!(to_snake_case("info.card"), "info_card");
    }

    #[test]
    fn snake_space_separator() {
        assert_eq!(to_snake_case("my component"), "my_component");
    }

    #[test]
    fn snake_consecutive_separators_collapsed() {
        assert_eq!(to_snake_case("a--b"), "a_b");
        assert_eq!(to_snake_case("a._b"), "a_b");
    }

    #[test]
    fn snake_leading_digit_prefixed() {
        assert_eq!(to_snake_case("3d"), "_3d");
        assert_eq!(to_snake_case("2fast"), "_2fast");
    }

    #[test]
    fn snake_strips_unknown_chars() {
        // chars that are not alphanumeric and not separators are silently dropped
        assert_eq!(to_snake_case("btn@primary"), "btnprimary");
    }

    // --- to_pascal_case ---

    #[test]
    fn pascal_basic_already_pascal() {
        assert_eq!(to_pascal_case("BtnPrimary"), "BtnPrimary");
        assert_eq!(to_pascal_case("HelloWorld"), "HelloWorld");
    }

    #[test]
    fn pascal_snake_input() {
        assert_eq!(to_pascal_case("btn_primary"), "BtnPrimary");
        assert_eq!(to_pascal_case("hello_world"), "HelloWorld");
    }

    #[test]
    fn pascal_hyphen_separator() {
        assert_eq!(to_pascal_case("my-component"), "MyComponent");
    }

    #[test]
    fn pascal_dot_separator() {
        assert_eq!(to_pascal_case("info.card"), "InfoCard");
        assert_eq!(to_pascal_case("btn.primary"), "BtnPrimary");
    }

    #[test]
    fn pascal_leading_digit_prefixed() {
        // digit cannot be uppercased; prefix with `_` so the identifier is valid
        assert_eq!(to_pascal_case("3d"), "_3d");
    }

    #[test]
    fn pascal_strips_non_alphanumeric_non_sep() {
        // `@` is not a separator, so no word boundary is introduced; `card` is appended as-is
        assert_eq!(to_pascal_case("info@card"), "Infocard");
    }

    #[test]
    fn pascal_single_word() {
        assert_eq!(to_pascal_case("primary"), "Primary");
        assert_eq!(to_pascal_case("card"), "Card");
    }
}

pub(crate) fn replace_whole_word(s: &str, from: &str, to: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut start = 0;
    while let Some(pos) = s[start..].find(from) {
        let abs = start + pos;
        let before_ok = abs == 0 || !is_ident_byte(bytes[abs - 1]);
        let after = abs + from.len();
        let after_ok = after >= bytes.len() || !is_ident_byte(bytes[after]);
        if before_ok && after_ok {
            result.push_str(&s[start..abs]);
            result.push_str(to);
            start = after;
        } else {
            result.push_str(&s[start..abs + 1]);
            start = abs + 1;
        }
    }
    result.push_str(&s[start..]);
    result
}
