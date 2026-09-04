//! Scanning helpers with no opinion about the language: where a paren closes, where a string ends, which bytes start a name.

pub(super) fn closing_paren(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut i = open;
    while i < bytes.len() {
        if let Some(end) = string_end(bytes, i) {
            i = end;
            continue;
        }
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// The index just past the string literal starting at `i`, or `None` when nothing starts there.
pub(super) fn string_end(bytes: &[u8], i: usize) -> Option<usize> {
    if bytes[i] != b'"' {
        return None;
    }
    let mut j = i + 1;
    while j < bytes.len() {
        match bytes[j] {
            b'\\' => j += 2,
            b'"' => return Some(j + 1),
            _ => j += 1,
        }
    }
    Some(bytes.len())
}

pub(super) fn top_level_space(s: &str) -> bool {
    let bytes = s.as_bytes();
    let (mut depth, mut i) = (0i32, 0usize);
    while i < s.len() {
        if let Some(end) = string_end(bytes, i) {
            i = end;
            continue;
        }
        match bytes[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            c if c.is_ascii_whitespace() && depth == 0 => return true,
            _ => {}
        }
        i += 1;
    }
    false
}

/// Whether a name may begin at `i`: not the tail of a longer one, not a field of something (`row.theme`), not already sigiled, and not a segment of a path (`crate::theme::…`).
pub(super) fn starts_a_name(bytes: &[u8], i: usize) -> bool {
    let prev = match i {
        0 => return true,
        _ => bytes[i - 1],
    };
    if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'.' || prev == b'$' {
        return false;
    }
    !(prev == b':' && i >= 2 && bytes[i - 2] == b':')
}

/// Applies `f` to every stretch of `body` outside a `"…"` literal — prose is not source, and a sentence mentioning the theme is not a read of it.
pub(super) fn replace_outside_strings(body: &str, f: impl Fn(&str) -> String) -> String {
    let bytes = body.as_bytes();
    let (mut out, mut chunk_at, mut i) = (String::with_capacity(body.len()), 0usize, 0usize);
    while i < body.len() {
        if let Some(end) = string_end(bytes, i) {
            out.push_str(&f(&body[chunk_at..i]));
            out.push_str(&body[i..end]);
            (chunk_at, i) = (end, end);
            continue;
        }
        i += 1;
    }
    out.push_str(&f(&body[chunk_at..]));
    out
}

/// The `>` closing the `<` at `open`, counting the ones between — and not the one in `->`, which is half an arrow and closes nothing.
pub(super) fn closing_angle(s: &str, open: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    for (i, c) in s.char_indices().skip_while(|(i, _)| *i < open) {
        match c {
            '<' => depth += 1,
            '>' if i == 0 || bytes[i - 1] != b'-' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// `from` → `to` wherever `from` is a whole name in a *value* position. A name followed by `:` is the attribute key, and a constant named `radius` used under a key of the same name is what makes that distinction load-bearing.
pub(super) fn replace_whole_name(chunk: &str, from: &str, to: &str) -> String {
    let bytes = chunk.as_bytes();
    let (mut out, mut i) = (String::with_capacity(chunk.len()), 0usize);
    while i < chunk.len() {
        let ends_ok = bytes
            .get(i + from.len())
            .is_none_or(|b| !(b.is_ascii_alphanumeric() || *b == b'_' || *b == b':'));
        if chunk[i..].starts_with(from) && starts_a_name(bytes, i) && ends_ok {
            out.push_str(to);
            i += from.len();
            continue;
        }
        let ch = chunk[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}
