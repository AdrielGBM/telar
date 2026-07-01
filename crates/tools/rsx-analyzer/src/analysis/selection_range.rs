//! `textDocument/selectionRange`: smart "expand selection" over a `.rsx` document.
//!
//! `.rsx` has no full AST with byte spans for every node, but its shape is regular enough to expand purely from structure: the identifier under the cursor → the line's content → each enclosing indentation block (so a `[view]` tree expands child → parent → grandparent) → the whole section → the document. Each step is strictly contained in the next, which is exactly the `SelectionRange` contract (`range` plus a `parent` pointing one level out).

use lsp_types::{Position, Range, SelectionRange};

use rsx_parser::{header_section, is_preview_header};

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
mod tests {
    use super::*;

    const SRC: &str = "[logic]\nlet count = signal(0i32);\n[view]\ncol @card\n    text \"Hi\"\n    row gap:8\n        btn \"+\"\n";

    /// Collects a hierarchy outermost→innermost as `(start_line, start_col)-(end_line, end_col)`.
    fn chain(node: &SelectionRange) -> Vec<((u32, u32), (u32, u32))> {
        let mut out = Vec::new();
        let mut cur = Some(node);
        while let Some(n) = cur {
            out.push((
                (n.range.start.line, n.range.start.character),
                (n.range.end.line, n.range.end.character),
            ));
            cur = n.parent.as_deref();
        }
        out.reverse();
        out
    }

    #[test]
    fn expands_word_to_line_to_block_to_section_to_document() {
        let lines: Vec<&str> = SRC.lines().collect();
        // Cursor on `btn` (line 6, deep in the [view] tree).
        let node = selection_for(
            &lines,
            Position {
                line: 6,
                character: 9,
            },
        );
        let c = chain(&node);
        // Outermost is the whole document; innermost is the `btn` word.
        assert_eq!(c.first().unwrap().0, (0, 0));
        let inner = c.last().unwrap();
        assert_eq!(inner.0, (6, 8));
        assert_eq!(inner.1, (6, 11));
        // The [view] section appears as a level, and at least one indentation block under `row`.
        assert!(c.iter().any(|&(s, _)| s == (2, 0)), "section level: {c:?}");
        // Strictly nested: each level contains the next.
        for w in c.windows(2) {
            let (outer, inner) = (w[0], w[1]);
            assert!(
                outer.0 <= inner.0 && inner.1 <= outer.1,
                "not nested: {c:?}"
            );
        }
    }

    // `[view]` followed by a parameterized `[preview …]` header.
    const WITH_PREVIEW: &str =
        "[view]\ncol\n    text \"Hi\"\n[preview \"Default\"]\ncounter\n    text \"x\"\n";

    #[test]
    fn the_view_section_stops_at_a_preview_header() {
        let lines: Vec<&str> = WITH_PREVIEW.lines().collect();
        // Cursor on `col` (line 1, a top-level [view] element).
        let c = chain(&selection_for(
            &lines,
            Position {
                line: 1,
                character: 0,
            },
        ));
        // The `[view]` section level ends at line 2 (before the preview header on line 3).
        assert!(
            c.iter().any(|&(start, end)| start == (0, 0) && end.0 == 2),
            "expected a [view] section ending at line 2: {c:?}"
        );
        // No level may span the `[view]` partially into the preview (only the whole-document level, ending at the last line, is allowed to start at line 0 and reach past line 2).
        assert!(
            !c.iter()
                .any(|&(start, end)| start.0 == 0 && (end.0 == 3 || end.0 == 4)),
            "a level swept part of the preview into the view selection: {c:?}"
        );
    }

    #[test]
    fn a_preview_is_its_own_section() {
        let lines: Vec<&str> = WITH_PREVIEW.lines().collect();
        // Cursor inside the preview body (`text "x"`, line 5).
        let c = chain(&selection_for(
            &lines,
            Position {
                line: 5,
                character: 4,
            },
        ));
        // Its enclosing section is the `[preview]` block (header on line 3 down to line 5), not the earlier `[view]`.
        assert!(
            c.iter().any(|&(start, end)| start == (3, 0) && end.0 == 5),
            "expected a [preview] section spanning lines 3..5: {c:?}"
        );
        assert!(
            !c.iter().any(|&(start, _)| start.0 < 3 && start != (0, 0)),
            "no level should reach back into the [view] section: {c:?}"
        );
    }

    #[test]
    fn block_groups_a_parent_with_its_children() {
        let lines: Vec<&str> = SRC.lines().collect();
        // The `row gap:8` block (line 5) should extend to include its `btn` child (line 6).
        let (start, end) = block_bounds(&lines, 5);
        assert_eq!((start, end), (5, 6));
    }
}
