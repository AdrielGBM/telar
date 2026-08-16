//! Range formatting for the language server.
//!
//! The formatter itself is [`telar_parser::format`] — whole-document, and shared with `cargo telar fmt` so the
//! editor and the command line cannot disagree. What is left here is the LSP-shaped half: turning that whole
//! document into the line hunks "Format Selection" and format-on-paste ask for.

pub use telar_parser::format::format_document;

// === range formatting ======================================================

/// Whole-line edits that turn `source` into its formatted form, restricted to the hunks overlapping `range`. The formatter is whole-document, so "Format Selection" / format-on-paste reuse it: we diff the formatted output against the source line-by-line and emit only the changed hunks that touch the requested range, leaving the rest of the file untouched.
pub fn range_edits(
    source: &str,
    formatted: &str,
    range: lsp_types::Range,
) -> Vec<lsp_types::TextEdit> {
    let src_lines: Vec<&str> = source.split('\n').collect();
    let fmt_lines: Vec<&str> = formatted.split('\n').collect();
    let starts = line_start_offsets(source);

    diff_hunks(&src_lines, &fmt_lines)
        .into_iter()
        .filter(|hunk| hunk_overlaps(hunk, &range))
        .map(|hunk| lsp_types::TextEdit {
            range: lsp_types::Range {
                start: crate::text::offset_to_position(source, starts[hunk.src_start]),
                end: crate::text::offset_to_position(source, starts[hunk.src_end]),
            },
            // Each replaced source line carried its trailing newline (the range ends at the start of the following line), so each replacement line keeps one too.
            new_text: fmt_lines[hunk.fmt_start..hunk.fmt_end]
                .iter()
                .map(|line| format!("{line}\n"))
                .collect(),
        })
        .collect()
}

/// A contiguous run of changed lines: source lines `[src_start, src_end)` become formatted lines `[fmt_start, fmt_end)`. A pure insertion has `src_start == src_end`; a pure deletion `fmt_start == fmt_end`.
struct Hunk {
    src_start: usize,
    src_end: usize,
    fmt_start: usize,
    fmt_end: usize,
}

/// Whether a hunk's source line span touches the requested line range (a pure insertion is a point).
fn hunk_overlaps(hunk: &Hunk, range: &lsp_types::Range) -> bool {
    let (a, b) = (hunk.src_start as u32, hunk.src_end as u32);
    let (lo, hi) = (range.start.line, range.end.line);
    if a == b {
        lo <= a && a <= hi
    } else {
        a <= hi && lo < b
    }
}

/// A standard LCS line diff, grouping the non-matching edits into hunks. Inputs are small (one file), so the O(n·m) table is fine.
fn diff_hunks(a: &[&str], b: &[&str]) -> Vec<Hunk> {
    let (n, m) = (a.len(), b.len());
    let mut lcs = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }

    let mut hunks = Vec::new();
    let mut current: Option<Hunk> = None;
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if a[i] == b[j] {
            flush(&mut current, &mut hunks);
            i += 1;
            j += 1;
        } else if lcs[i + 1][j] >= lcs[i][j + 1] {
            current
                .get_or_insert(Hunk {
                    src_start: i,
                    src_end: i,
                    fmt_start: j,
                    fmt_end: j,
                })
                .src_end = i + 1;
            i += 1;
        } else {
            current
                .get_or_insert(Hunk {
                    src_start: i,
                    src_end: i,
                    fmt_start: j,
                    fmt_end: j,
                })
                .fmt_end = j + 1;
            j += 1;
        }
    }
    if i < n || j < m {
        let hunk = current.get_or_insert(Hunk {
            src_start: i,
            src_end: i,
            fmt_start: j,
            fmt_end: j,
        });
        hunk.src_end = n;
        hunk.fmt_end = m;
    }
    flush(&mut current, &mut hunks);
    hunks
}

fn flush(current: &mut Option<Hunk>, hunks: &mut Vec<Hunk>) {
    if let Some(hunk) = current.take() {
        hunks.push(hunk);
    }
}

/// Byte offset where each line starts; `offsets[i]` is line `i`'s start and the final entry is the end of the document, so a half-open line span `[s, e)` maps to bytes `offsets[s]..offsets[e]`.
fn line_start_offsets(source: &str) -> Vec<usize> {
    let mut offsets = vec![0usize];
    let mut running = 0usize;
    for part in source.split('\n') {
        running += part.len() + 1; // +1 for the '\n'
        offsets.push(running);
    }
    offsets.pop(); // the last split piece has no trailing '\n'
    offsets.push(source.len());
    offsets
}

/// A byte offset within `source` → LSP `(line, UTF-16 col)`.
#[cfg(test)]
mod tests {
    use super::*;

    fn point(line: u32) -> lsp_types::Range {
        lsp_types::Range {
            start: lsp_types::Position { line, character: 0 },
            end: lsp_types::Position { line, character: 0 },
        }
    }

    #[test]
    fn range_edits_only_touch_hunks_in_the_requested_range() {
        // Two independent changes — line 1 and line 3 — formatting fixes both.
        let src = "a\nXX\nb\nYY\nc\n";
        let formatted = "a\nxx\nb\nyy\nc\n";
        let edits = range_edits(src, formatted, point(1));
        // Only the line-1 hunk is emitted; the line-3 change is left for a separate request.
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].range.start.line, 1);
        assert_eq!(edits[0].range.end.line, 2);
        assert_eq!(edits[0].new_text, "xx\n");
    }

    #[test]
    fn range_edits_are_empty_when_the_range_has_no_change() {
        let src = "a\nXX\nb\n";
        let formatted = "a\nxx\nb\n";
        // Line 0 (`a`) is unchanged; its range yields no edits even though the document differs.
        assert!(range_edits(src, formatted, point(0)).is_empty());
    }

    #[test]
    fn range_edits_handle_an_insertion_hunk() {
        // The formatter added a blank line between two classes; request the seam.
        let src = "@a\n@b\n";
        let formatted = "@a\n\n@b\n";
        let edits = range_edits(src, formatted, point(1));
        assert_eq!(edits.len(), 1);
        // A pure insertion is a zero-width edit at the start of line 1.
        assert_eq!(edits[0].range.start, edits[0].range.end);
        assert_eq!(edits[0].new_text, "\n");
    }
}
