//! Scanning hand-written Rust: what is code and what is a literal, so an identifier search does not match inside a string.
//!
//! Split from the naming conventions in [`telar_project::naming`]: those say what a generated name looks like and are read by the macro that places the output, while these read the `[logic]` Rust an author wrote and only codegen does that.

use telar_project::naming::is_ident_byte;

/// If `bytes[i]` opens a string, raw string, char literal or comment, returns the index just past it, so an identifier scan skips its contents — a name embedded in `"text"` or `// note` is not a real reference to it. A `'a` lifetime tick (no closing quote) is left alone; escaped char literals (`'\n'`) are handled. Shared by [`contains_ident`] and [`replace_whole_word`] so both agree on what is code.
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

#[cfg(all(test, feature = "transpile"))]
#[path = "lexer_test.rs"]
mod tests;

/// Replaces every whole-word occurrence of identifier `from` with `to`, leaving string/char literals and line comments untouched (a `from` inside `"..."` or after `//` is not an identifier, so rewriting it would corrupt the text). Skipping the same regions as [`contains_ident`] keeps detection and rewrite in agreement. A struct literal's field name is skipped for the same reason — see [`is_struct_field_name`].

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
            && !is_field_access(bytes, i)
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

/// Whether the identifier at `start` names a field in a struct literal (`Config { volume: volume.peek() }`) rather than a binding. Renaming it there produces a struct that has no such field — which is what a form's save closure writes on nearly every line, since a field and the signal holding it want the same name. Whether this identifier sits behind a `.`, which makes it a field or a method and never a variable.
///
/// The clone rewrite renames a captured binding wherever it is *read*, and a read is a name standing on its own — `store().tool` names a field of whatever `store()` returned, not the local called `tool`. Renaming it produced a `no field \`tool_rsx_mv\`` pointing at generated code the author never wrote, and it took a local and a field merely sharing a name, which in an application store is the normal case rather than the odd one. `..` is excluded because a struct-update spread (`Config { ..base }`) or a range really does read the binding.

fn is_field_access(bytes: &[u8], start: usize) -> bool {
    start > 0 && bytes[start - 1] == b'.' && !(start > 1 && bytes[start - 2] == b'.')
}

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
