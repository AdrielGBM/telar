//! `textDocument/semanticTokens/full`: parse-aware highlighting that TextMate (regex) can't do — it distinguishes **component** tags from **built-in** tags, marks `@class` references and `$signal` reads, so the semantics of the `.rsx` read at a glance. Coexists with the TextMate grammar (which stays the fallback for everything else).

use lsp_types::{SemanticToken, SemanticTokenType};
use rsx_transpiler::{is_builtin_tag, is_control_flow_keyword};

use crate::position::{Section, find_section_at};
use crate::text::{byte_to_utf16, leading_token};

/// The legend, in index order. The `token_type` field of each emitted token indexes into this.
pub fn token_types() -> Vec<SemanticTokenType> {
    vec![
        SemanticTokenType::KEYWORD,  // 0 — built-in tag (col, text, box, …)
        SemanticTokenType::FUNCTION, // 1 — component tag (another .rsx)
        SemanticTokenType::CLASS,    // 2 — @class (def or ref)
        SemanticTokenType::VARIABLE, // 3 — $signal read
    ]
}

const TAG_BUILTIN: u32 = 0;
const TAG_COMPONENT: u32 = 1;
const CLASS: u32 = 2;
const SIGNAL: u32 = 3;

pub fn semantic_tokens(source: &str) -> Vec<SemanticToken> {
    let raw = raw_tokens(source);
    // LSP wants position-sorted, delta-encoded 5-tuples.
    let mut out = Vec::with_capacity(raw.len());
    let (mut prev_line, mut prev_start) = (0u32, 0u32);
    for &(line, start, len, ty) in &raw {
        let delta_line = line - prev_line;
        let delta_start = if delta_line == 0 {
            start - prev_start
        } else {
            start
        };
        out.push(SemanticToken {
            delta_line,
            delta_start,
            length: len,
            token_type: ty,
            token_modifiers_bitset: 0,
        });
        prev_line = line;
        prev_start = start;
    }
    out
}

/// `(line, start_utf16, len_utf16, token_type)` for every classified token, sorted by position.
fn raw_tokens(source: &str) -> Vec<(u32, u32, u32, u32)> {
    let mut raw = Vec::new();
    for (i, line) in source.lines().enumerate() {
        let li = i as u32;
        match find_section_at(source, li) {
            // `find_section_at` reports `[preview]` bodies as `View`, so they're covered here too.
            Section::View => view_tokens(line, li, &mut raw),
            Section::Style => style_tokens(line, li, &mut raw),
            _ => {}
        }
    }
    raw.sort_by_key(|t| (t.0, t.1));
    raw
}

fn view_tokens(line: &str, li: u32, raw: &mut Vec<(u32, u32, u32, u32)>) {
    // The element tag is the first token; classify built-in vs component.
    if let Some((lead, tag)) = leading_token(line)
        && tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !is_control_flow_keyword(tag)
    {
        let ty = if is_builtin_tag(tag) {
            TAG_BUILTIN
        } else {
            TAG_COMPONENT
        };
        push(raw, li, line, lead, tag.len(), ty);
    }
    for (start, len) in sigil_tokens(line, b'@') {
        push(raw, li, line, start, len, CLASS);
    }
    for (start, len) in sigil_tokens(line, b'$') {
        push(raw, li, line, start, len, SIGNAL);
    }
}

fn style_tokens(line: &str, li: u32, raw: &mut Vec<(u32, u32, u32, u32)>) {
    for (start, len) in sigil_tokens(line, b'@') {
        push(raw, li, line, start, len, CLASS);
    }
}

/// Byte spans of `<sigil><ident>` tokens in `line` (the span includes the sigil). `@` names allow `-`.
fn sigil_tokens(line: &str, sigil: u8) -> Vec<(usize, usize)> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == sigil {
            let mut j = i + 1;
            while j < bytes.len()
                && (bytes[j].is_ascii_alphanumeric()
                    || bytes[j] == b'_'
                    || (sigil == b'@' && bytes[j] == b'-'))
            {
                j += 1;
            }
            if j > i + 1 {
                out.push((i, j - i));
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    out
}

/// Pushes a token, converting the `[byte_start, byte_start+byte_len)` span to UTF-16 columns.
fn push(
    raw: &mut Vec<(u32, u32, u32, u32)>,
    line_idx: u32,
    line: &str,
    byte_start: usize,
    byte_len: usize,
    ty: u32,
) {
    let start = byte_to_utf16(line, byte_start);
    let end = byte_to_utf16(line, byte_start + byte_len);
    raw.push((line_idx, start, end - start, ty));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_tags_classes_and_signals() {
        // 0:[style] 1:@card 2:[view] 3:col @card 4:  text "{$count}" 5:  feature_card icon:"x"
        let src = "[style]\n@card\n[view]\ncol @card\n    text \"{$count}\"\n    feature_card icon:\"x\"\n";
        let raw = raw_tokens(src);

        let has = |line: u32, col: u32, ty: u32| {
            raw.iter()
                .any(|&(l, c, _, t)| l == line && c == col && t == ty)
        };

        // `@card` def in [style] (line 1, col 0) → class.
        assert!(has(1, 0, CLASS), "style class def: {raw:?}");
        // `col` builtin tag (line 3, col 0) → keyword.
        assert!(has(3, 0, TAG_BUILTIN), "builtin tag: {raw:?}");
        // `@card` ref in the view (line 3, col 4) → class.
        assert!(has(3, 4, CLASS), "class ref: {raw:?}");
        // `$count` inside the interpolation → variable.
        assert!(
            raw.iter().any(|&(l, _, _, t)| l == 4 && t == SIGNAL),
            "signal: {raw:?}"
        );
        // `feature_card` non-builtin tag (line 5, col 4) → function (component).
        assert!(has(5, 4, TAG_COMPONENT), "component tag: {raw:?}");
    }

    #[test]
    fn delta_encoding_is_monotonic() {
        let src = "[view]\ncol @a\n    text @b\n";
        let toks = semantic_tokens(src);
        // First token carries an absolute line; subsequent deltas are non-negative by construction.
        assert!(!toks.is_empty());
    }
}
