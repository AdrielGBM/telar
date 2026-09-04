//! Shared `@class` / `$signal` / component-tag occurrence finders, powering document-highlight, references and rename. Style classes and signals are file-scoped, so a single source scan is complete. `[logic]` is skipped for `@class` (a `@` there is Rust, not a class). Returned ranges cover the name (after any sigil), so a rename replaces the name and leaves the sigil.

use lsp_types::Range;
use telar_transpiler::{is_builtin_tag, is_control_flow_keyword};

use crate::position::{Section, find_section_at};
use crate::text::{ident_at, leading_token, name_range, utf16_to_byte};

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

/// The component tag under the cursor and its byte start: the leading token of a `[view]`/`[preview]` line, when it is a plain identifier that is neither a built-in tag nor a control-flow keyword (i.e. a reference to another `.rsx`). `None` for built-ins, keywords and attribute positions.
fn component_token(source: &str, line: u32, character: u32) -> Option<(usize, &str)> {
    if !matches!(
        find_section_at(source, line),
        Section::View | Section::Preview
    ) {
        return None;
    }
    let line_text = source.lines().nth(line as usize)?;
    let (lead, token) = leading_token(line_text)?;
    let cursor = utf16_to_byte(line_text, character);
    if cursor < lead || cursor > lead + token.len() {
        return None;
    }
    if !token.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        || is_control_flow_keyword(token)
        || is_builtin_tag(token)
    {
        return None;
    }
    Some((lead, token))
}

/// The component tag under the cursor, if it is on one.
pub fn component_at(source: &str, line: u32, character: u32) -> Option<String> {
    component_token(source, line, character).map(|(_, token)| token.to_string())
}

/// The name-range of the component tag under the cursor, for `prepareRename`.
pub fn component_at_range(source: &str, line: u32, character: u32) -> Option<Range> {
    let line_text = source.lines().nth(line as usize)?;
    let (lead, token) = component_token(source, line, character)?;
    Some(name_range(line, line_text, lead, token.len()))
}

/// The signal name under the cursor: a `$name` in `[view]`, or a `name` in `[logic]` declared as a signal/memo. A signal is file-scoped (`[logic]` declaration + uses, `$name` in `[view]`).
///
/// NOTE: the `[logic]` side is a whole-word scan, not a rust-analyzer-precise resolve — robust for the usual distinct signal names, but it would also touch a same-named local in `[logic]`.
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
        Section::Logic => ident_at(line_text, character)?.1.to_string(),
        _ => return None,
    };
    is_declared_signal(source, &name).then_some(name)
}

/// Every occurrence of signal `name`: whole-word in `[logic]` (declaration + uses) and `$name` (the name range, past the `$`) in `[view]`/`[preview]`.
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

/// Each `@ident` token in a line: the byte offset of `ident` (past the `@`) and its text. Class names match the parser's permissive set: alphanumerics, `_` and `-`.
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

/// Names of all signals/memos declared in the `[logic]` section, for completion.
pub fn declared_signals(source: &str) -> Vec<String> {
    let mut logic = String::new();
    for (i, line) in source.lines().enumerate() {
        if find_section_at(source, i as u32) == Section::Logic {
            logic.push_str(line);
            logic.push('\n');
        }
    }
    telar_transpiler::scan_signals(&logic)
        .into_iter()
        .map(|s| s.name)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "[style]\n@card\n    width: 240\n[view]\ncol @card\n    box @card\n";

    #[test]
    fn class_at_recognizes_def_and_ref() {
        assert_eq!(class_at(SRC, 1, 2).as_deref(), Some("card"));
        assert_eq!(class_at(SRC, 4, 6).as_deref(), Some("card"));
        assert_eq!(class_at(SRC, 2, 5), None);
    }

    #[test]
    fn occurrences_cover_def_and_all_refs() {
        let ranges = class_occurrences(SRC, "card");
        let lines: Vec<u32> = ranges.iter().map(|r| r.start.line).collect();
        assert_eq!(lines, vec![1, 4, 5]);
        for r in &ranges {
            assert_eq!(r.end.character - r.start.character, 4);
        }
    }

    #[test]
    fn component_at_recognizes_non_builtin_tags_only() {
        let src = "[view]\ncol\n    feature_card icon:\"x\"\n    text \"hi\"\n";
        assert_eq!(component_at(src, 2, 6).as_deref(), Some("feature_card"));
        assert_eq!(component_at(src, 1, 0), None);
        assert_eq!(component_at(src, 3, 4), None);
    }

    #[test]
    fn signals_resolve_across_logic_and_view() {
        let src =
            "[logic]\nlet count = signal(0i32);\nlet x = 5;\n[view]\ncol\n    text \"{$count}\"\n";
        assert_eq!(signal_at(src, 1, 5).as_deref(), Some("count"));
        assert_eq!(signal_at(src, 5, 13).as_deref(), Some("count"));
        assert_eq!(signal_at(src, 2, 4), None);
        let lines: Vec<u32> = signal_occurrences(src, "count")
            .iter()
            .map(|r| r.start.line)
            .collect();
        assert!(lines.contains(&1) && lines.contains(&5), "lines: {lines:?}");
    }

    #[test]
    fn declared_signals_lists_logic_signals() {
        let src = "[logic]\nlet count = signal(0i32);\nlet double = memo(|| 0);\nlet x = 5;\n[view]\ncol\n";
        let mut names = declared_signals(src);
        names.sort();
        assert_eq!(names, vec!["count".to_string(), "double".to_string()]);
    }

    #[test]
    fn logic_at_signs_are_ignored() {
        let src = "[logic]\nlet x = foo(\"@card\");\n[view]\ncol @card\n";
        let ranges = class_occurrences(src, "card");
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start.line, 3);
    }
}
