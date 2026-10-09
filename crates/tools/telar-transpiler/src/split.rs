//! The one splitter every value parser here shares: cut at a boundary character, but only where no bracket, paren or string literal is open.

/// `text` cut at each character `is_boundary` accepts outside any `(...)`, `[...]` or `"..."`, so a separator nested in an argument list, a list or a text stays in the segment it is part of. `keep_empty` keeps the empty segments between adjacent boundaries and after a trailing one.
pub(crate) fn split_top_level(
    text: &str,
    is_boundary: impl Fn(char) -> bool,
    keep_empty: bool,
) -> Vec<&str> {
    let mut segments = Vec::new();
    let mut depth = 0i32;
    let mut quoted = false;
    let mut escaped = false;
    let mut start = 0;
    for (at, c) in text.char_indices() {
        if quoted {
            match (escaped, c) {
                (true, _) => escaped = false,
                (false, '\\') => escaped = true,
                (false, '"') => quoted = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => quoted = true,
            '[' | '(' => depth += 1,
            ']' | ')' => depth -= 1,
            c if depth == 0 && is_boundary(c) => {
                if keep_empty || at > start {
                    segments.push(&text[start..at]);
                }
                start = at + c.len_utf8();
            }
            _ => {}
        }
    }
    if keep_empty || start < text.len() {
        segments.push(&text[start..]);
    }
    segments
}

#[cfg(test)]
#[path = "split_test.rs"]
mod tests;
