//! `textDocument/foldingRange`: collapsible regions for `.rsx`.
//!
//! Two kinds, derived straight from the source text (so it works even when the document does not parse): **section folds** collapse each `[logic]`/`[style]`/`[view]`/`[preview …]` block, and **indentation folds** collapse nested `[view]` elements and multi-line `[style]` classes.

use lsp_types::{FoldingRange, FoldingRangeKind};
use telar_parser::{header_section, is_preview_header};

/// The foldable regions: each section, and each indentation block inside it.
pub fn folding_ranges(source: &str) -> Vec<FoldingRange> {
    let lines: Vec<&str> = source.lines().collect();
    let mut ranges = Vec::new();
    section_folds(&lines, &mut ranges);
    indentation_folds(&lines, &mut ranges);
    ranges
}

/// Whether a line is a section header (`[logic]`/`[style]`/`[view]` or a `[preview …]`).
fn is_header(line: &str) -> bool {
    let t = line.trim();
    header_section(t).is_some() || is_preview_header(t)
}

/// One fold per section: from its header line to the last non-blank line before the next header.
fn section_folds(lines: &[&str], ranges: &mut Vec<FoldingRange>) {
    let headers: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| is_header(l))
        .map(|(i, _)| i)
        .collect();

    for (k, &start) in headers.iter().enumerate() {
        let next = headers.get(k + 1).copied().unwrap_or(lines.len());
        let end = (start + 1..next)
            .rev()
            .find(|&j| !lines[j].trim().is_empty());
        if let Some(end) = end
            && end > start
        {
            ranges.push(fold(start, end, Some(FoldingRangeKind::Region)));
        }
    }
}

/// One fold per line that opens a deeper-indented block, ending at the last line still inside it.
fn indentation_folds(lines: &[&str], ranges: &mut Vec<FoldingRange>) {
    let indent = |l: &str| {
        if l.trim().is_empty() {
            None
        } else {
            Some(l.len() - l.trim_start().len())
        }
    };

    for i in 0..lines.len() {
        let Some(di) = indent(lines[i]) else {
            continue;
        };
        let mut end = i;
        let mut j = i + 1;
        while j < lines.len() {
            match indent(lines[j]) {
                Some(dj) if dj > di => {
                    end = j;
                    j += 1;
                }
                // Dedent back to this level or shallower closes the block.
                Some(_) => break,
                // Blank lines don't extend the fold but don't close it either.
                None => j += 1,
            }
        }
        if end > i {
            ranges.push(fold(i, end, None));
        }
    }
}

fn fold(start: usize, end: usize, kind: Option<FoldingRangeKind>) -> FoldingRange {
    FoldingRange {
        start_line: start as u32,
        start_character: None,
        end_line: end as u32,
        end_character: None,
        kind,
        collapsed_text: None,
    }
}

#[cfg(test)]
#[path = "folding_test.rs"]
mod tests;
