//! `textDocument/selectionRange`: smart "expand selection" over a `.rsx` document.
//!
//! `.rsx` has no full AST with byte spans for every node, but its shape is regular enough to expand purely from structure: the identifier under the cursor → the line's content → each enclosing indentation block (so a `[view]` tree expands child → parent → grandparent) → the whole section → the document. Each step is strictly contained in the next, which is exactly the `SelectionRange` contract (`range` plus a `parent` pointing one level out).

use lsp_types::{Position, Range, SelectionRange};

use telar_parser::{header_section, is_preview_header};

use crate::text::{byte_to_utf16, ident_at, name_range, utf16_len};

/// Whether a trimmed line opens a section: the fixed `[logic]`/`[style]`/`[view]` headers plus the parameterized `[preview "Name" …]` header (so a preview is its own selectable section and never gets swept into the preceding `[view]` block).
fn is_section_header(trimmed: &str) -> bool {
    header_section(trimmed).is_some() || is_preview_header(trimmed)
}

/// One `SelectionRange` hierarchy per requested position (LSP sends a batch).
pub fn selection_ranges(source: &str, positions: &[Position]) -> Vec<SelectionRange> {
    let lines: Vec<&str> = source.lines().collect();
    positions
        .iter()
        .map(|p| selection_for(&lines, *p))
        .collect()
}

fn selection_for(lines: &[&str], pos: Position) -> SelectionRange {
    // Outermost → innermost; consecutive duplicates and non-containing entries are dropped below.
    let mut ranges: Vec<Range> = Vec::new();

    ranges.push(document_range(lines));
    if let Some(section) = section_range(lines, pos.line) {
        ranges.push(section);
    }
    for block in indentation_blocks(lines, pos.line as usize) {
        ranges.push(block);
    }
    if let Some(line_text) = lines.get(pos.line as usize) {
        if let Some(content) = line_content_range(pos.line, line_text) {
            ranges.push(content);
        }
        if let Some(word) = word_range(pos.line, line_text, pos.character) {
            ranges.push(word);
        }
    }

    build(ranges, pos)
}

/// Folds outer→inner ranges into a nested [`SelectionRange`], keeping only entries that are strictly inside the one before them (so a degenerate/empty step never breaks the monotonic chain). Always yields at least the cursor position itself.
fn build(ranges: Vec<Range>, pos: Position) -> SelectionRange {
    let mut node: Option<SelectionRange> = None;
    let mut last: Option<Range> = None;
    for range in ranges {
        if let Some(prev) = last
            && (range == prev || !contains(prev, range))
        {
            continue;
        }
        last = Some(range);
        node = Some(SelectionRange {
            range,
            parent: node.map(Box::new),
        });
    }
    node.unwrap_or(SelectionRange {
        range: Range {
            start: pos,
            end: pos,
        },
        parent: None,
    })
}

/// Whether `inner` is contained within `outer` (equal bounds count as contained).
fn contains(outer: Range, inner: Range) -> bool {
    !before(inner.start, outer.start) && !before(outer.end, inner.end)
}

fn before(a: Position, b: Position) -> bool {
    (a.line, a.character) < (b.line, b.character)
}

fn document_range(lines: &[&str]) -> Range {
    let last = lines.len().saturating_sub(1) as u32;
    let last_len = lines.last().map(|l| utf16_len(l)).unwrap_or(0);
    Range {
        start: Position {
            line: 0,
            character: 0,
        },
        end: Position {
            line: last,
            character: last_len,
        },
    }
}

/// The `[section]` block the cursor sits in: from its header line down to the line before the next header (or end of file). `None` before the first header.
fn section_range(lines: &[&str], line: u32) -> Option<Range> {
    let target = line as usize;
    let mut start = None;
    for (i, text) in lines.iter().enumerate() {
        if is_section_header(text.trim()) {
            if i <= target {
                start = Some(i);
            } else if start.is_some() {
                return Some(full_lines(lines, start.unwrap(), i - 1));
            }
        }
    }
    start.map(|s| full_lines(lines, s, lines.len().saturating_sub(1)))
}

/// The chain of enclosing indentation blocks for `line`, innermost first: the line plus its more-indented descendants, then each shallower ancestor with its own descendants. Section headers are never crossed.
fn indentation_blocks(lines: &[&str], line: usize) -> Vec<Range> {
    if line >= lines.len() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut cursor = line;
    loop {
        let (start, end) = block_bounds(lines, cursor);
        out.push(full_lines(lines, start, end));
        let base = indent(lines[cursor]);
        let mut parent = None;
        for k in (0..cursor).rev() {
            let text = lines[k];
            if text.trim().is_empty() {
                continue;
            }
            if is_section_header(text.trim()) {
                break;
            }
            if indent(text) < base {
                parent = Some(k);
                break;
            }
        }
        match parent {
            Some(k) => cursor = k,
            None => break,
        }
    }
    out.reverse();
    out
}

/// The block rooted at `line`: itself plus the following run of deeper-indented (or blank) lines, trimmed back to the last non-blank descendant.
fn block_bounds(lines: &[&str], line: usize) -> (usize, usize) {
    let base = indent(lines[line]);
    let mut end = line;
    let mut j = line + 1;
    while j < lines.len() {
        let text = lines[j];
        if text.trim().is_empty() {
            j += 1;
            continue;
        }
        if is_section_header(text.trim()) || indent(text) <= base {
            break;
        }
        end = j;
        j += 1;
    }
    (line, end)
}

/// A range spanning whole lines `start..=end`.
fn full_lines(lines: &[&str], start: usize, end: usize) -> Range {
    Range {
        start: Position {
            line: start as u32,
            character: 0,
        },
        end: Position {
            line: end as u32,
            character: lines.get(end).map(|l| utf16_len(l)).unwrap_or(0),
        },
    }
}

/// The line's trimmed content (first non-whitespace to end-of-line); `None` for a blank line.
fn line_content_range(line: u32, text: &str) -> Option<Range> {
    let lead = text.len() - text.trim_start().len();
    if text.trim().is_empty() {
        return None;
    }
    Some(Range {
        start: Position {
            line,
            character: byte_to_utf16(text, lead),
        },
        end: Position {
            line,
            character: utf16_len(text),
        },
    })
}

/// The identifier (alphanumerics + `_`) under the UTF-16 cursor. `None` when not on a word.
fn word_range(line: u32, text: &str, character: u32) -> Option<Range> {
    let (start, word) = ident_at(text, character)?;
    Some(name_range(line, text, start, word.len()))
}

fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

#[cfg(test)]
#[path = "selection_range_test.rs"]
mod tests;
