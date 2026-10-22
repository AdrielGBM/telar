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

/// `expr` with every closure that a callee known to keep it (see [`crate::registry::CLOSURE_KEEPING_CALLEES`]) is handed blanked out, so what is left is what runs when the expression is evaluated; such a closure's body only runs when its owner calls it. Every other closure stays: a callee the transpiler cannot see into (`std::array::from_fn(|i| …)`, a helper, an adapter) may call it on the spot, and a read missed there freezes silently. Same length as `expr`, so offsets still line up.
pub(crate) fn without_deferred_closures(expr: &str) -> String {
    let bytes = expr.as_bytes();
    let mut blanked = bytes.to_vec();
    let mut keeps: Vec<bool> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if let Some(end) = literal_or_comment_end(bytes, i) {
            i = end;
            continue;
        }
        match bytes[i] {
            b'(' => {
                let before = expr[..i].trim_end();
                let callee =
                    before.trim_end_matches(|c: char| c.is_alphanumeric() || c == '_' || c == ':');
                keeps.push(crate::registry::keeps_closure(&before[callee.len()..]));
            }
            b'[' | b'{' => keeps.push(false),
            b')' | b']' | b'}' => {
                keeps.pop();
            }
            _ => {}
        }
        let Some(params_end) = closure_params_end(expr, i) else {
            i += 1;
            continue;
        };
        if !keeps.last().copied().unwrap_or(false) {
            i = params_end;
            continue;
        }
        let end = closure_body_end(expr, params_end);
        blanked[i..end].fill(b' ');
        i = end;
    }
    String::from_utf8(blanked).unwrap_or_else(|_| expr.to_string())
}

/// The index just past the parameter list of a closure starting at `at` (`|a, b|`, `||`, `move |a|`), or `None` when `at` starts no closure; a `|` after an operand is a binary or.
fn closure_params_end(expr: &str, at: usize) -> Option<usize> {
    let bytes = expr.as_bytes();
    let mut bar = at;
    if expr[at..].starts_with("move")
        && (at == 0 || !is_ident_byte(bytes[at - 1]))
        && !bytes.get(at + 4).is_some_and(|&b| is_ident_byte(b))
    {
        let rest = &expr[at + 4..];
        bar = at + 4 + (rest.len() - rest.trim_start().len());
    } else if let Some(prev) = expr[..at].trim_end().chars().next_back()
        && (prev.is_alphanumeric() || matches!(prev, '_' | ')' | ']' | '}' | '"' | '\''))
    {
        return None;
    }
    if bytes.get(bar) != Some(&b'|') {
        return None;
    }
    if bytes.get(bar + 1) == Some(&b'|') {
        return Some(bar + 2);
    }
    let close = expr[bar + 1..].find('|')?;
    Some(bar + close + 2)
}

/// The index where the closure body that starts at `from` ends: at the first `,` or `;` outside brackets, or at the bracket that closes the list it sits in.
fn closure_body_end(expr: &str, from: usize) -> usize {
    let bytes = expr.as_bytes();
    let mut depth = 0usize;
    let mut i = from;
    while i < bytes.len() {
        if let Some(end) = literal_or_comment_end(bytes, i) {
            i = end;
            continue;
        }
        match bytes[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => match depth {
                0 => return i,
                _ => depth -= 1,
            },
            b',' | b';' if depth == 0 => return i,
            _ => {}
        }
        i += 1;
    }
    bytes.len()
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
