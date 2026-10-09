//! Shared `@class` / `$signal` / component-tag occurrence finders, powering document-highlight, references and rename. Style classes and signals are file-scoped, so a single source scan is complete. `[logic]`, `[play]` and the `[previews]` prose are skipped for `@class` (a `@` there is Rust or prose, not a class). Returned ranges cover the name (after any sigil), so a rename replaces the name and leaves the sigil.

use lsp_types::Range;
use telar_transpiler::{is_builtin_tag, is_control_flow_keyword};

use crate::analysis::preview_header::{self, TokenKind};
use crate::position::{Section, find_section_at};
use crate::text::{ident_at, leading_token, name_range, utf16_to_byte};
use telar_parser::{header_section, section_opened_by};

/// The class name under the cursor, if it sits on a `@name` token (on the `@` or anywhere in `name`).
pub fn class_at(source: &str, line: u32, character: u32) -> Option<String> {
    if find_section_at(source, line).is_verbatim() {
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

/// Every `@name` occurrence's name-range across the document (skipping `[logic]`, `[play]` and prose).
pub fn class_occurrences(source: &str, name: &str) -> Vec<Range> {
    let mut out = Vec::new();
    for (line_idx, line_text) in source.lines().enumerate() {
        if find_section_at(source, line_idx as u32).is_verbatim() {
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

/// A signal a `$name` can read: file-scoped when declared in `[logic]`, local to one preview when declared by that preview's `args(…)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signal {
    pub name: String,
    preview: Option<usize>,
}

/// The signal under the cursor: a `$name` in `[view]` or a preview body, a `name` in `[logic]` declared as a signal/memo, or an `args(…)` name in a preview header. An arg shadows a `[logic]` signal of the same name inside its own preview.
///
/// NOTE: the `[logic]` side is a whole-word scan, not a rust-analyzer-precise resolve — robust for the usual distinct signal names, but it would also touch a same-named local in `[logic]`.
pub fn signal_at(source: &str, line: u32, character: u32) -> Option<Signal> {
    let line_text = source.lines().nth(line as usize)?;
    let scopes = preview_scopes(source);
    let scope = scopes.get(line as usize).copied().flatten();
    match find_section_at(source, line) {
        Section::View => {
            let cursor = utf16_to_byte(line_text, character);
            let name = dollar_idents(line_text)
                .into_iter()
                .find(|(pos, n)| cursor >= *pos && cursor <= *pos + 1 + n.len())
                .map(|(_, n)| n)?;
            resolve_read(source, scope, name)
        }
        Section::Logic => {
            let name = ident_at(line_text, character)?.1.to_string();
            is_declared_signal(source, &name).then_some(Signal {
                name,
                preview: None,
            })
        }
        Section::Preview => {
            let (start, len) = arg_name_at(line_text, utf16_to_byte(line_text, character))?;
            Some(Signal {
                name: line_text[start..start + len].to_string(),
                preview: scope,
            })
        }
        _ => None,
    }
}

/// Every occurrence of `signal`. A `[logic]` signal: whole-word in `[logic]` (declaration + uses) and `$name` in `[view]` and the preview bodies that do not declare an arg of that name. An arg: its name in the header and `$name` in that preview's body. Ranges cover the name, past the `$`.
pub fn signal_occurrences(source: &str, signal: &Signal) -> Vec<Range> {
    let scopes = preview_scopes(source);
    let name = signal.name.as_str();
    let mut out = Vec::new();
    for (i, line) in source.lines().enumerate() {
        let li = i as u32;
        let scope = scopes[i];
        match find_section_at(source, li) {
            Section::Logic if signal.preview.is_none() => {
                for (start, len) in whole_word_positions(line, name) {
                    out.push(name_range(li, line, start, len));
                }
            }
            Section::View if reads_signal(source, scope, signal) => {
                for (pos, n) in dollar_idents(line) {
                    if n == name {
                        out.push(name_range(li, line, pos + 1, name.len()));
                    }
                }
            }
            Section::Preview if signal.preview == Some(i) => {
                for (start, len) in arg_names(line) {
                    if &line[start..start + len] == name {
                        out.push(name_range(li, line, start, len));
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
        Section::Preview => {
            let (start, len) = arg_name_at(line_text, utf16_to_byte(line_text, character))?;
            Some(name_range(line, line_text, start, len))
        }
        _ => None,
    }
}

/// The header line of the `[preview …]` each line belongs to, `None` outside one. Everything up to the next header is the preview's body.
fn preview_scopes(source: &str) -> Vec<Option<usize>> {
    let mut current = None;
    source
        .lines()
        .enumerate()
        .map(|(i, line)| {
            let trimmed = line.trim();
            if header_section(trimmed).is_some() {
                current = None;
            } else if section_opened_by(trimmed) == Some(Section::View) {
                current = Some(i);
            } else if section_opened_by(trimmed).is_some() {
                current = None;
            }
            current
        })
        .collect()
}

/// The byte spans of the names `args(…)` declares on a preview header line.
fn arg_names(line: &str) -> Vec<(usize, usize)> {
    preview_header::header_tokens(line)
        .into_iter()
        .filter(|token| token.kind == TokenKind::ArgName)
        .map(|token| (token.start, token.len))
        .collect()
}

fn arg_name_at(line: &str, cursor: usize) -> Option<(usize, usize)> {
    arg_names(line)
        .into_iter()
        .find(|(start, len)| cursor >= *start && cursor <= start + len)
}

fn declares_arg(source: &str, header: usize, name: &str) -> bool {
    source.lines().nth(header).is_some_and(|line| {
        arg_names(line)
            .into_iter()
            .any(|(start, len)| &line[start..start + len] == name)
    })
}

fn resolve_read(source: &str, scope: Option<usize>, name: String) -> Option<Signal> {
    if let Some(header) = scope.filter(|header| declares_arg(source, *header, &name)) {
        return Some(Signal {
            name,
            preview: Some(header),
        });
    }
    is_declared_signal(source, &name).then_some(Signal {
        name,
        preview: None,
    })
}

/// Whether a `$name` on a line of `scope` reads `signal`: the preview's own arg, or a `[logic]` signal no arg of that preview shadows.
fn reads_signal(source: &str, scope: Option<usize>, signal: &Signal) -> bool {
    match signal.preview {
        Some(header) => scope == Some(header),
        None => !scope.is_some_and(|header| declares_arg(source, header, &signal.name)),
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
#[path = "occurrences_test.rs"]
mod tests;
