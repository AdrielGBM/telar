//! Scanning hand-written Rust: what is code and what is a literal, so an identifier search does not match inside a string.
//!
//! Split from the naming conventions in [`telar_project::naming`]: those say what a generated name looks like and are read by the macro that places the output, while these read the `[logic]` Rust an author wrote and only codegen does that.

use telar_project::naming::is_ident_byte;

/// If `bytes[i]` opens a string, raw string, char literal or comment, returns the index just past it, so an identifier scan skips its contents — a name embedded in `"text"` or `// note` is not a real reference to it. A `'a` lifetime tick (no closing quote) is left alone; escaped char literals (`'\n'`) are handled. Shared by every identifier scan here, so they agree on what is code.
///
/// A `//` ends at its newline, not at the end of the input: these scanners run over whole `[logic]` blocks, where swallowing the rest of the snippet would hide every signal declared after the first comment.
pub(crate) fn literal_or_comment_end(bytes: &[u8], i: usize) -> Option<usize> {
    match bytes[i] {
        b'/' if bytes.get(i + 1) == Some(&b'/') => Some(
            bytes[i..]
                .iter()
                .position(|&b| b == b'\n')
                .map_or(bytes.len(), |n| i + n),
        ),
        b'/' if bytes.get(i + 1) == Some(&b'*') => {
            let mut j = i + 2;
            while j + 1 < bytes.len() && !(bytes[j] == b'*' && bytes[j + 1] == b'/') {
                j += 1;
            }
            Some((j + 2).min(bytes.len()))
        }
        // `r"…"` / `r#"…"#` have no escapes, so the terminator is the quote plus as many `#` as opened it.
        b'r' if matches!(bytes.get(i + 1), Some(&b'"') | Some(&b'#')) => {
            let hashes = bytes[i + 1..].iter().take_while(|&&b| b == b'#').count();
            if bytes.get(i + 1 + hashes) != Some(&b'"') {
                return None;
            }
            let mut j = i + 2 + hashes;
            while j < bytes.len() {
                if bytes[j] == b'"'
                    && bytes[j + 1..].iter().take_while(|&&b| b == b'#').count() >= hashes
                {
                    return Some((j + 1 + hashes).min(bytes.len()));
                }
                j += 1;
            }
            Some(bytes.len())
        }
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

/// Whether `code` references `ident` as a whole-word identifier, skipping string/char literals and line comments (a name that appears only inside `"..."` or after `//` is not a reference).
pub(crate) fn contains_ident(code: &str, ident: &str) -> bool {
    ident_positions(code, ident).next().is_some()
}

/// Byte offset of every whole-word `ident` in `code` outside literals and comments, in source order.
pub(crate) fn ident_positions<'a>(
    code: &'a str,
    ident: &'a str,
) -> impl Iterator<Item = usize> + 'a {
    let bytes = code.as_bytes();
    let mut i = 0;
    std::iter::from_fn(move || {
        while i < bytes.len() {
            if let Some(end) = literal_or_comment_end(bytes, i) {
                i = end;
                continue;
            }
            let at = i;
            i += code[at..].chars().next().map_or(1, char::len_utf8);
            if code[at..].starts_with(ident)
                && (at == 0 || !is_ident_byte(bytes[at - 1]))
                && bytes
                    .get(at + ident.len())
                    .is_none_or(|&b| !is_ident_byte(b))
            {
                return Some(at);
            }
        }
        None
    })
}

/// Byte offset of every `ident` inside a string literal of `code` that stands where a format string names an argument — `{ident}`, `{ident:…}`, or a `ident$` width or precision. Whether the string is a format string at all is for the caller to decide; this only finds the candidates.
pub(crate) fn format_arg_positions(code: &str, ident: &str) -> Vec<usize> {
    let bytes = code.as_bytes();
    let mut positions = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let Some(end) = literal_or_comment_end(bytes, i) else {
            i += 1;
            continue;
        };
        if matches!(bytes[i], b'"' | b'r') {
            let literal = &code[i..end];
            for (at, _) in literal.match_indices(ident) {
                let before = at.checked_sub(1).map(|b| literal.as_bytes()[b]);
                let after = literal.as_bytes().get(at + ident.len()).copied();
                let named = before == Some(b'{') && matches!(after, Some(b'}' | b':'));
                let counted = before.is_some_and(|b| !is_ident_byte(b)) && after == Some(b'$');
                if named || counted {
                    positions.push(i + at);
                }
            }
        }
        i = end;
    }
    positions
}

#[cfg(all(test, feature = "transpile"))]
#[path = "lexer_test.rs"]
mod tests;
