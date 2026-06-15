//! Shared identifier conversions between RSX names and generated Rust names.

/// Converts an RSX name (`card-title`, `primary`) into a snake_case identifier (`card_title`, `primary`).
pub fn to_snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        match c {
            '-' | ' ' => out.push('_'),
            c if c.is_ascii_alphanumeric() => out.push(c.to_ascii_lowercase()),
            '_' => out.push('_'),
            _ => {}
        }
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
