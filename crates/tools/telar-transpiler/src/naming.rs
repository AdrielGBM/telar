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

pub(crate) fn is_ident_byte(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphanumeric()
}

/// Returns true if `s` is a valid Rust identifier: starts with `_` or a letter, rest `_`/alphanumeric.
pub(crate) fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c == '_' || c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

/// If `bytes[i]` opens a string/char literal or a `//` line comment, returns the index just past it, so an
/// identifier scan skips its contents — a name embedded in `"text"` or `// note` is not a real reference to
/// it. A `'a` lifetime tick (no closing quote) is left alone; escaped char literals (`'\n'`) are handled.
/// Shared by [`contains_ident`] and [`crate::naming::replace_whole_word`] so both agree on what is code.
pub(crate) fn literal_or_comment_end(bytes: &[u8], i: usize) -> Option<usize> {
    match bytes[i] {
        b'/' if bytes.get(i + 1) == Some(&b'/') => Some(bytes.len()),
        b'"' => {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] != b'"' {
                j += if bytes[j] == b'\\' { 2 } else { 1 };
            }
            Some((j + 1).min(bytes.len()))
        }
        b'\'' if bytes.get(i + 1) == Some(&b'\\') => {
            let mut j = i + 2;
            while j < bytes.len() && bytes[j] != b'\'' {
                j += 1;
            }
            Some((j + 1).min(bytes.len()))
        }
        b'\'' if bytes.get(i + 2) == Some(&b'\'') => Some(i + 3),
        _ => None,
    }
}

/// Whether `code` references `ident` as a whole-word identifier, skipping string/char literals and line
/// comments (a name that appears only inside `"..."` or after `//` is not a reference).
pub(crate) fn contains_ident(code: &str, ident: &str) -> bool {
    let bytes = code.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if let Some(end) = literal_or_comment_end(bytes, i) {
            i = end;
            continue;
        }
        if code[i..].starts_with(ident)
            && (i == 0 || !is_ident_byte(bytes[i - 1]))
            && bytes
                .get(i + ident.len())
                .is_none_or(|&b| !is_ident_byte(b))
        {
            return true;
        }
        let ch = code[i..].chars().next().unwrap();
        i += ch.len_utf8();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn contains_ident_skips_literals_and_comments() {
        assert!(contains_ident("charging.get()", "charging"));
        assert!(
            !contains_ident("charging_glyph.get()", "charging"),
            "prefix is not a whole word"
        );
        // The reported bug: a signal name embedded in a string literal is not a reference.
        assert!(!contains_ident(
            "if c { \"battery-charging\" } else { \"x\" }",
            "charging"
        ));
        assert!(
            !contains_ident("x + 1 // reset charging", "charging"),
            "comment is not code"
        );
        // A char literal must not hide a real following reference.
        assert!(contains_ident(
            "if c == 'x' { charging.set(true) }",
            "charging"
        ));
    }

    #[test]
    fn replace_whole_word_leaves_literals_and_comments_intact() {
        assert_eq!(
            replace_whole_word("charging.get()", "charging", "c2"),
            "c2.get()"
        );
        // The string literal keeps its `charging`; only the real identifier is renamed.
        assert_eq!(
            replace_whole_word("charging = \"battery-charging\"", "charging", "c2"),
            "c2 = \"battery-charging\""
        );
        assert_eq!(
            replace_whole_word("charging.set(0) // charging", "charging", "c2"),
            "c2.set(0) // charging"
        );
        // A prefix must not be renamed.
        assert_eq!(
            replace_whole_word("charging_glyph", "charging", "c2"),
            "charging_glyph"
        );
    }

    #[test]
    fn replace_whole_word_leaves_a_struct_literal_field_alone() {
        // The shape every form's save closure has: a field and the signal holding it share a name, and the
        // clone rewrite used to rename both — leaving a struct literal naming a field that does not exist.
        assert_eq!(
            replace_whole_word("Config { vim: vim.peek() }", "vim", "vim_rsx_mv"),
            "Config { vim: vim_rsx_mv.peek() }"
        );
        assert_eq!(
            replace_whole_word("C { a: 1, vim: vim.peek() }", "vim", "v2"),
            "C { a: 1, vim: v2.peek() }"
        );
        // A type annotation is not a field name, and a path is not a colon.
        assert_eq!(
            replace_whole_word("let vim: bool = vim.peek();", "vim", "v2"),
            "let v2: bool = v2.peek();"
        );
        assert_eq!(replace_whole_word("vim::set()", "vim", "v2"), "v2::set()");
    }
}

/// Replaces every whole-word occurrence of identifier `from` with `to`, leaving string/char literals and
/// line comments untouched (a `from` inside `"..."` or after `//` is not an identifier, so rewriting it
/// would corrupt the text). Skipping the same regions as [`contains_ident`] keeps detection and rewrite in
/// agreement. A struct literal's field name is skipped for the same reason — see [`is_struct_field_name`].
pub(crate) fn replace_whole_word(s: &str, from: &str, to: &str) -> String {
    let bytes = s.as_bytes();
    let mut result = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if let Some(end) = literal_or_comment_end(bytes, i) {
            result.push_str(&s[i..end]);
            i = end;
            continue;
        }
        if s[i..].starts_with(from)
            && (i == 0 || !is_ident_byte(bytes[i - 1]))
            && bytes.get(i + from.len()).is_none_or(|&b| !is_ident_byte(b))
            && !is_struct_field_name(bytes, i, from.len())
        {
            result.push_str(to);
            i += from.len();
        } else {
            let ch = s[i..].chars().next().unwrap();
            result.push(ch);
            i += ch.len_utf8();
        }
    }
    result
}

/// Whether the identifier at `start` names a field in a struct literal (`Config { volume: volume.peek() }`)
/// rather than a binding. Renaming it there produces a struct that has no such field — which is what a form's
/// save closure writes on nearly every line, since a field and the signal holding it want the same name.
fn is_struct_field_name(bytes: &[u8], start: usize, len: usize) -> bool {
    let mut after = start + len;
    while bytes.get(after).is_some_and(u8::is_ascii_whitespace) {
        after += 1;
    }
    if bytes.get(after) != Some(&b':') || bytes.get(after + 1) == Some(&b':') {
        return false;
    }
    let mut before = start;
    while before > 0 && bytes[before - 1].is_ascii_whitespace() {
        before -= 1;
    }
    before > 0 && matches!(bytes[before - 1], b'{' | b',')
}
