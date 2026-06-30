//! Shared `@class` occurrence finder powering document-highlight, references and rename.
//!
//! Style classes are file-scoped (defined in `[style]`, used in `[view]`/`[preview]`), so every
//! occurrence lives in one document and a source scan is complete. `[logic]` is skipped — a `@` there
//! is Rust, not a class. Returned ranges cover the *name* (after the `@`), so a rename replaces the
//! name and leaves the sigil.

use lsp_types::{Position, Range};

use crate::position::{Section, find_section_at};

/// The class name under the cursor, if it sits on a `@name` token (on the `@` or anywhere in `name`).
pub fn class_at(source: &str, line: u32, character: u32) -> Option<String> {
    if matches!(
        find_section_at(source, line),
        Section::Logic | Section::Unknown
    ) {
        return None;
    }
    let line_text = source.lines().nth(line as usize)?;
    let cursor = utf16_to_byte(line_text, character);
    for (name_start, name) in scan_class_tokens(line_text) {
        // Inclusive of the trailing edge so the cursor just past the name still counts.
        if cursor >= name_start - 1 && cursor <= name_start + name.len() {
            return Some(name.to_string());
        }
    }
    None
}

/// Every `@name` occurrence's name-range across the document (skipping `[logic]`).
pub fn class_occurrences(source: &str, name: &str) -> Vec<Range> {
    let mut out = Vec::new();
    for (line_idx, line_text) in source.lines().enumerate() {
        if matches!(
            find_section_at(source, line_idx as u32),
            Section::Logic | Section::Unknown
        ) {
            continue;
        }
        for (name_start, token) in scan_class_tokens(line_text) {
            if token == name {
                out.push(name_range(
                    line_idx as u32,
                    line_text,
                    name_start,
                    token.len(),
                ));
            }
        }
    }
    out
}

/// The name-range of the specific `@name` token under the cursor (for `prepareRename`).
pub fn occurrence_at(source: &str, line: u32, character: u32) -> Option<Range> {
    let line_text = source.lines().nth(line as usize)?;
    let cursor = utf16_to_byte(line_text, character);
    for (name_start, name) in scan_class_tokens(line_text) {
        if cursor >= name_start - 1 && cursor <= name_start + name.len() {
            return Some(name_range(line, line_text, name_start, name.len()));
        }
    }
    None
}

/// The component name under the cursor: the first token (element tag) of a `[view]`/`[preview]` line,
/// when it is a plain identifier that is not a built-in tag or control-flow keyword (i.e. a reference
/// to another `.rsx`). Returns `None` for built-ins (`col`, `text`, …) and `@class`/attribute positions.
pub fn component_at(source: &str, line: u32, character: u32) -> Option<String> {
    if !matches!(
        find_section_at(source, line),
        Section::View | Section::Preview
    ) {
        return None;
    }
    let line_text = source.lines().nth(line as usize)?;
    let lead = line_text.len() - line_text.trim_start().len();
    let token = line_text[lead..]
        .split(|c: char| c.is_whitespace())
        .next()?;
    if token.is_empty() {
        return None;
    }
    let cursor = utf16_to_byte(line_text, character);
    if cursor < lead || cursor > lead + token.len() {
        return None;
    }
    if !token.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        || matches!(token, "if" | "for" | "let" | "else")
        || rsx_transpiler::builtin_tags()
            .iter()
            .any(|(t, _)| *t == token)
    {
        return None;
    }
    Some(token.to_string())
}

/// The name-range of the component tag under the cursor, for `prepareRename`. Assumes the caller has
/// already gated on [`component_at`] (it does not re-check that the tag is a real, non-builtin tag);
/// it only recomputes the leading tag token's range.
pub fn component_at_range(source: &str, line: u32, character: u32) -> Option<Range> {
    if !matches!(
        find_section_at(source, line),
        Section::View | Section::Preview
    ) {
        return None;
    }
    let line_text = source.lines().nth(line as usize)?;
    let lead = line_text.len() - line_text.trim_start().len();
    let token = line_text[lead..]
        .split(|c: char| c.is_whitespace())
        .next()?;
    if token.is_empty() {
        return None;
    }
    let cursor = utf16_to_byte(line_text, character);
    if cursor < lead || cursor > lead + token.len() {
        return None;
    }
    Some(name_range(line, line_text, lead, token.len()))
}

/// The signal name under the cursor: a `$name` in `[view]`, or a `name` identifier in `[logic]` that
/// is declared as a signal/memo. Returns `None` otherwise. A signal is file-scoped (declared in
/// `[logic]`, used as `name` there and `$name` in `[view]`).
///
/// NOTE: the `[logic]` side is a whole-word scan, not a rust-analyzer-precise resolve — robust for the
/// usual distinct signal names, but it would also touch a same-named local in `[logic]`.
pub fn signal_at(source: &str, line: u32, character: u32) -> Option<String> {
    let line_text = source.lines().nth(line as usize)?;
    let name = match find_section_at(source, line) {
        Section::View => {
            let cursor = utf16_to_byte(line_text, character);
            dollar_idents(line_text)
                .into_iter()
                .find(|(pos, n)| cursor >= *pos && cursor <= *pos + 1 + n.len())
                .map(|(_, n)| n)?
        }
        Section::Logic => ident_at(line_text, character)?.1,
        _ => return None,
    };
    is_declared_signal(source, &name).then_some(name)
}

/// Every occurrence of signal `name`: whole-word in `[logic]` (declaration + uses) and `$name` (the
/// name range, past the `$`) in `[view]`/`[preview]`.
pub fn signal_occurrences(source: &str, name: &str) -> Vec<Range> {
    let mut out = Vec::new();
    for (i, line) in source.lines().enumerate() {
        let li = i as u32;
        match find_section_at(source, li) {
            Section::Logic => {
                for (start, len) in whole_word_positions(line, name) {
                    out.push(name_range(li, line, start, len));
                }
            }
            Section::View => {
                for (pos, n) in dollar_idents(line) {
                    if n == name {
                        out.push(name_range(li, line, pos + 1, name.len()));
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// The name-range of the signal token under the cursor, for `prepareRename`.
pub fn signal_occurrence_at(source: &str, line: u32, character: u32) -> Option<Range> {
    let line_text = source.lines().nth(line as usize)?;
    match find_section_at(source, line) {
        Section::View => {
            let cursor = utf16_to_byte(line_text, character);
            dollar_idents(line_text)
                .into_iter()
                .find(|(pos, n)| cursor >= *pos && cursor <= *pos + 1 + n.len())
                .map(|(pos, n)| name_range(line, line_text, pos + 1, n.len()))
        }
        Section::Logic => {
            let (start, word) = ident_at(line_text, character)?;
            Some(name_range(line, line_text, start, word.len()))
        }
        _ => None,
    }
}

/// Whether `name` is declared as a signal/memo (`let name = signal(…)` / `memo(…)`) in `[logic]`.
fn is_declared_signal(source: &str, name: &str) -> bool {
    for (i, line) in source.lines().enumerate() {
        if find_section_at(source, i as u32) != Section::Logic {
            continue;
        }
        let Some(rest) = line.trim().strip_prefix("let ") else {
            continue;
        };
        let rest = rest.strip_prefix("mut ").unwrap_or(rest);
        let Some((binding, expr)) = rest.split_once('=') else {
            continue;
        };
        let bind = binding.trim().split(':').next().unwrap_or("").trim();
        let expr = expr.trim_start();
        if bind == name && (expr.starts_with("signal(") || expr.starts_with("memo(")) {
            return true;
        }
    }
    false
}

/// The identifier (alphanumerics + `_`) surrounding the UTF-16 cursor, with its byte start.
fn ident_at(line: &str, character: u32) -> Option<(usize, String)> {
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
    if start == end {
        return None;
    }
    Some((start, line[start..end].to_string()))
}

/// `(byte offset of `$`, ident)` for each `$ident` in `line`.
fn dollar_idents(line: &str) -> Vec<(usize, String)> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            if j > start {
                out.push((i, line[start..j].to_string()));
            }
            i = j.max(start);
        } else {
            i += 1;
        }
    }
    out
}

/// Byte spans of `name` as a whole word in `line` (boundaries are non-identifier characters).
fn whole_word_positions(line: &str, name: &str) -> Vec<(usize, usize)> {
    let (bytes, nb) = (line.as_bytes(), name.as_bytes());
    if nb.is_empty() {
        return Vec::new();
    }
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut out = Vec::new();
    let mut i = 0;
    while i + nb.len() <= bytes.len() {
        if &bytes[i..i + nb.len()] == nb
            && (i == 0 || !is_ident(bytes[i - 1]))
            && (i + nb.len() == bytes.len() || !is_ident(bytes[i + nb.len()]))
        {
            out.push((i, nb.len()));
            i += nb.len();
        } else {
            i += 1;
        }
    }
    out
}

/// Finds each `@ident` token in a line, returning the byte offset of `ident` (past the `@`) and the
/// `ident` text. Class names match the parser's permissive set: alphanumerics, `_`, and `-`.
fn scan_class_tokens(line: &str) -> Vec<(usize, &str)> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'@' {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len()
                && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_' || bytes[j] == b'-')
            {
                j += 1;
            }
            if j > start {
                out.push((start, &line[start..j]));
            }
            i = j.max(start);
        } else {
            i += 1;
        }
    }
    out
}

fn name_range(line: u32, line_text: &str, name_start: usize, len: usize) -> Range {
    Range {
        start: Position {
            line,
            character: byte_to_utf16(line_text, name_start),
        },
        end: Position {
            line,
            character: byte_to_utf16(line_text, name_start + len),
        },
    }
}

fn utf16_to_byte(line: &str, utf16_col: u32) -> usize {
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

fn byte_to_utf16(line: &str, byte_col: usize) -> u32 {
    line[..byte_col.min(line.len())].encode_utf16().count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "[style]\n@card\n    width: 240\n[view]\ncol @card\n    box @card\n";

    #[test]
    fn class_at_recognizes_def_and_ref() {
        // Cursor on `card` in the `[style]` def (line 1, col 2).
        assert_eq!(class_at(SRC, 1, 2).as_deref(), Some("card"));
        // Cursor on `card` in the `[view]` ref (line 4, `col @card`, col 6).
        assert_eq!(class_at(SRC, 4, 6).as_deref(), Some("card"));
        // Cursor in `[style]` but not on a class.
        assert_eq!(class_at(SRC, 2, 5), None);
    }

    #[test]
    fn occurrences_cover_def_and_all_refs() {
        let ranges = class_occurrences(SRC, "card");
        // def (line 1) + two refs (lines 4, 5).
        let lines: Vec<u32> = ranges.iter().map(|r| r.start.line).collect();
        assert_eq!(lines, vec![1, 4, 5]);
        // Each range covers just the name (`card`, 4 cols).
        for r in &ranges {
            assert_eq!(r.end.character - r.start.character, 4);
        }
    }

    #[test]
    fn component_at_recognizes_non_builtin_tags_only() {
        let src = "[view]\ncol\n    feature_card icon:\"x\"\n    text \"hi\"\n";
        // Cursor on the component tag.
        assert_eq!(component_at(src, 2, 6).as_deref(), Some("feature_card"));
        // Built-in tags are not components.
        assert_eq!(component_at(src, 1, 0), None);
        assert_eq!(component_at(src, 3, 4), None);
    }

    #[test]
    fn signals_resolve_across_logic_and_view() {
        let src =
            "[logic]\nlet count = signal(0i32);\nlet x = 5;\n[view]\ncol\n    text \"{$count}\"\n";
        // `count` is a signal (recognized on its decl and on `$count`); `x` is a plain local.
        assert_eq!(signal_at(src, 1, 5).as_deref(), Some("count"));
        assert_eq!(signal_at(src, 5, 13).as_deref(), Some("count"));
        assert_eq!(signal_at(src, 2, 4), None);
        // Occurrences span the [logic] decl and the [view] `$count`.
        let lines: Vec<u32> = signal_occurrences(src, "count")
            .iter()
            .map(|r| r.start.line)
            .collect();
        assert!(lines.contains(&1) && lines.contains(&5), "lines: {lines:?}");
    }

    #[test]
    fn logic_at_signs_are_ignored() {
        let src = "[logic]\nlet x = foo(\"@card\");\n[view]\ncol @card\n";
        // The `@card` inside the [logic] string must not be an occurrence.
        let ranges = class_occurrences(src, "card");
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start.line, 3);
    }
}
