//! The ids Rust names a component-named asset kind by: a macro called as the component is, `icon!("mdi:home")`, in a crate's `.rs` files or in the Rust a `.rsx` writes.
//!
//! The source is read by a lexer of its own rather than `proc-macro2`'s: a position out of that one needs its `span-locations`, which keeps the text of every parse for the life of the process, and the editor bakes on every save.

use std::path::{Path, PathBuf};

use telar_project::AssetKind;

use crate::ids::IdRef;

/// Every `tag!("id")` in `source`, where `tag` is the component of one of `kinds`, as read from `file` with `source` starting on its `first_line`.
pub(crate) fn collect_macro_ids(
    source: &str,
    file: &Path,
    first_line: usize,
    kinds: &[&'static AssetKind],
    out: &mut Vec<IdRef>,
) {
    let tags: Vec<(&str, &'static AssetKind)> = kinds
        .iter()
        .filter_map(|kind| Some((kind.component?.tag, *kind)))
        .filter(|(tag, _)| source.contains(tag))
        .collect();
    if tags.is_empty() {
        return;
    }
    for window in lex(source).windows(5) {
        let [
            Token::Ident(name, line),
            Token::Punct('!'),
            Token::Open,
            Token::Str(written),
            Token::Close,
        ] = window
        else {
            continue;
        };
        let Some(&(_, kind)) = tags.iter().find(|(tag, _)| tag == name) else {
            continue;
        };
        let Some(id) = string_value(written) else {
            continue;
        };
        out.push(IdRef {
            kind,
            tag: name.to_string(),
            prop: kind.attr.to_string(),
            literal: Some(id.trim().to_string()),
            written: written.to_string(),
            file: file.to_path_buf(),
            line: first_line + line - 1,
        });
    }
}

/// Every Rust source file in the package at `package_dir`, outside `target/` and dot-directories such as `.telar/`, sorted.
pub(crate) fn rust_files(package_dir: &Path) -> Vec<PathBuf> {
    telar_project::collect_files_by_ext(package_dir, &["rs"], &|name| {
        name != "target" && !name.starts_with('.')
    })
}

#[derive(Debug, PartialEq)]
enum Token<'a> {
    /// An identifier and its 1-based line.
    Ident(&'a str, usize),
    Punct(char),
    Open,
    Close,
    /// A string literal as written, prefix and quotes included.
    Str(&'a str),
    /// Any other literal, a lifetime or a label.
    Other,
}

/// `source` as the tokens that matter to finding a macro call, with comments dropped and every literal kept whole, so nothing inside a comment, a string or a character reads as a call.
fn lex(source: &str) -> Vec<Token<'_>> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut line = 1;
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        let byte = bytes[i];
        let literal = prefixed_literal_end(bytes, i);
        if let Some(end) = literal {
            i = end;
            let text = &source[start..i];
            tokens.push(match text.ends_with('"') || text.ends_with('#') {
                true => Token::Str(text),
                false => Token::Other,
            });
        } else if byte.is_ascii_whitespace() {
            i += 1;
        } else if bytes[i..].starts_with(b"//") {
            i = bytes[i..]
                .iter()
                .position(|&b| b == b'\n')
                .map_or(bytes.len(), |at| i + at);
        } else if bytes[i..].starts_with(b"/*") {
            i = block_comment_end(bytes, i);
        } else if byte == b'"' {
            i = quoted_end(bytes, i + 1);
            tokens.push(Token::Str(&source[start..i]));
        } else if byte == b'\'' {
            i = quote_or_lifetime_end(source, i);
            tokens.push(Token::Other);
        } else if is_ident_byte(byte) && !byte.is_ascii_digit() {
            i = ident_end(bytes, i);
            tokens.push(Token::Ident(&source[start..i], line));
        } else if byte.is_ascii_digit() {
            i += 1;
            while i < bytes.len()
                && (is_ident_byte(bytes[i])
                    || (bytes[i] == b'.' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit)))
            {
                i += 1;
            }
            tokens.push(Token::Other);
        } else {
            i += 1;
            tokens.push(match byte {
                b'(' | b'[' | b'{' => Token::Open,
                b')' | b']' | b'}' => Token::Close,
                _ => Token::Punct(char::from(byte)),
            });
        }
        line += source[start..i].matches('\n').count();
    }
    tokens
}

fn is_ident_byte(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphanumeric() || byte >= 0x80
}

fn ident_end(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && is_ident_byte(bytes[i]) {
        i += 1;
    }
    i
}

/// Where the block comment opening at `i` ends, nested ones included.
fn block_comment_end(bytes: &[u8], mut i: usize) -> usize {
    let mut depth = 0usize;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"/*") {
            depth += 1;
            i += 2;
        } else if bytes[i..].starts_with(b"*/") {
            depth -= 1;
            i += 2;
            if depth == 0 {
                return i;
            }
        } else {
            i += 1;
        }
    }
    i
}

/// Where a `"`-delimited literal whose body starts at `i` ends, past its closing quote and any suffix.
fn quoted_end(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return ident_end(bytes, i + 1),
            _ => i += 1,
        }
    }
    bytes.len()
}

/// Where the character literal, lifetime or label starting with the `'` at `i` ends.
fn quote_or_lifetime_end(source: &str, i: usize) -> usize {
    let rest = &source[i + 1..];
    let mut chars = rest.char_indices();
    let end = match (chars.next(), chars.next()) {
        (Some((_, '\\')), _) => rest
            .get(2..)
            .and_then(|tail| tail.find('\''))
            .map_or(rest.len(), |at| at + 3),
        (Some(_), Some((at, '\''))) => at + 1,
        (Some((_, first)), _) => rest
            .find(|c: char| !(c == '_' || c.is_alphanumeric()))
            .unwrap_or(rest.len())
            .max(first.len_utf8()),
        (None, _) => rest.len(),
    };
    i + 1 + end
}

/// Where a literal with a letter prefix starting at `i` ends: a raw string (`r"…"`, `r#"…"#`), a byte or C string (`b"…"`, `br"…"`, `c"…"`) or a byte (`b'x'`). `None` when `i` starts an identifier, a raw one (`r#type`) included.
fn prefixed_literal_end(bytes: &[u8], i: usize) -> Option<usize> {
    let (body, raw) = match (bytes[i], bytes.get(i + 1)) {
        (b'b' | b'c', Some(b'r')) => (i + 2, true),
        (b'r', _) => (i + 1, true),
        (b'b' | b'c', _) => (i + 1, false),
        _ => return None,
    };
    match bytes.get(body) {
        Some(b'"') if !raw => Some(quoted_end(bytes, body + 1)),
        Some(b'\'') if !raw && bytes[i] == b'b' => {
            let mut j = body + 1;
            while j < bytes.len() && bytes[j] != b'\'' {
                j += if bytes[j] == b'\\' { 2 } else { 1 };
            }
            Some((j + 1).min(bytes.len()))
        }
        Some(b'"' | b'#') if raw => {
            let hashes = bytes[body..].iter().take_while(|&&b| b == b'#').count();
            if bytes.get(body + hashes) != Some(&b'"') {
                return None;
            }
            let fence = |j: usize| {
                bytes
                    .get(j + 1..j + 1 + hashes)
                    .is_some_and(|tail| tail.iter().all(|&b| b == b'#'))
            };
            let close = (body + hashes + 1..bytes.len()).find(|&j| bytes[j] == b'"' && fence(j));
            Some(close.map_or(bytes.len(), |j| j + 1 + hashes))
        }
        _ => None,
    }
}

/// The value of a string literal as written, `"mdi:home"` or `r#"mdi:home"#`. `None` for a byte or C string, a suffixed one, and one holding an escape, which no id needs.
fn string_value(written: &str) -> Option<&str> {
    if let Some(raw) = written.strip_prefix('r') {
        let hashes = raw.len() - raw.trim_start_matches('#').len();
        let fence = &raw[..hashes];
        return raw[hashes..]
            .strip_prefix('"')?
            .strip_suffix(fence)?
            .strip_suffix('"');
    }
    let value = written.strip_prefix('"')?.strip_suffix('"')?;
    (!value.contains('\\')).then_some(value)
}

#[cfg(test)]
#[path = "macro_ids_test.rs"]
mod tests;
