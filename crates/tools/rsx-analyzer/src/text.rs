//! Shared text/position primitives. LSP columns are UTF-16 code units, so every byte↔column conversion lives here once instead of being re-derived per analysis module.

use lsp_types::{Position, Range};

/// Byte offset of a UTF-16 column within a single line (clamped to the line end).
pub fn utf16_to_byte(line: &str, utf16_col: u32) -> usize {
    let mut remaining = utf16_col;
    let mut byte = 0;
    for ch in line.chars() {
        let w = ch.len_utf16() as u32;
        if remaining < w {
            break;
        }
        remaining -= w;
        byte += ch.len_utf8();
    }
    byte
}

/// UTF-16 column of a byte offset within a single line.
pub fn byte_to_utf16(line: &str, byte_col: usize) -> u32 {
    line[..byte_col.min(line.len())].encode_utf16().count() as u32
}

/// UTF-16 length of a line.
pub fn utf16_len(line: &str) -> u32 {
    line.encode_utf16().count() as u32
}

/// LSP range over the `[start, start+len)` byte span on a single line.
pub fn name_range(line: u32, line_text: &str, start: usize, len: usize) -> Range {
    Range {
        start: Position {
            line,
            character: byte_to_utf16(line_text, start),
        },
        end: Position {
            line,
            character: byte_to_utf16(line_text, start + len),
        },
    }
}

/// The identifier (alphanumerics + `_`) around the UTF-16 cursor, with its byte start. `None` off a word.
pub fn ident_at(line: &str, character: u32) -> Option<(usize, &str)> {
    let cursor = utf16_to_byte(line, character).min(line.len());
    let bytes = line.as_bytes();
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut start = cursor;
    while start > 0 && is_ident(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = cursor;
    while end < bytes.len() && is_ident(bytes[end]) {
        end += 1;
    }
    (start != end).then(|| (start, &line[start..end]))
}

/// The leading element token of a line (first whitespace-delimited word) and its byte start (the indent width). `None` for a blank line.
pub fn leading_token(line: &str) -> Option<(usize, &str)> {
    let lead = line.len() - line.trim_start().len();
    let token = line[lead..].split(|c: char| c.is_whitespace()).next()?;
    (!token.is_empty()).then_some((lead, token))
}

/// LSP position (0-based line, UTF-16 column) of a byte offset within multi-line `source` (clamped).
pub fn offset_to_position(source: &str, byte: usize) -> Position {
    let byte = byte.min(source.len());
    let mut line = 0u32;
    let mut line_start = 0usize;
    for (i, _) in source[..byte].match_indices('\n') {
        line += 1;
        line_start = i + 1;
    }
    Position {
        line,
        character: source[line_start..byte].encode_utf16().count() as u32,
    }
}

/// Byte offset of the `(line, utf16_col)` cursor within multi-line `source`, on a UTF-8 char boundary.
pub fn byte_offset(source: &str, line: u32, utf16_col: u32) -> Option<usize> {
    let mut line_start = 0usize;
    for (i, current) in source.split_inclusive('\n').enumerate() {
        if i as u32 == line {
            let content = current.strip_suffix('\n').unwrap_or(current);
            return Some(line_start + utf16_to_byte(content, utf16_col));
        }
        line_start += current.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ident_at_finds_the_word_under_the_cursor() {
        let line = "let total_count = 1;";
        let (start, word) = ident_at(line, 7).unwrap();
        assert_eq!((start, word), (4, "total_count"));
        // Cursor on the `=` (col 16) → no identifier.
        assert!(ident_at(line, 16).is_none());
    }

    #[test]
    fn leading_token_skips_indentation() {
        assert_eq!(leading_token("    btn \"+\""), Some((4, "btn")));
        assert_eq!(leading_token("   "), None);
    }

    #[test]
    fn offset_and_byte_offset_round_trip_multibyte() {
        // `é` is 2 UTF-8 bytes / 1 UTF-16 unit; the cursor after it is byte 3, col 2 on line 1.
        let src = "[view]\ncol é x\n";
        let byte = byte_offset(src, 1, 6).unwrap();
        let pos = offset_to_position(src, byte);
        assert_eq!((pos.line, pos.character), (1, 6));
    }
}
