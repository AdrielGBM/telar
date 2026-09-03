//! Line breaking in character cells.
//!
//! One implementation, used by both the measurer layout asks and the painter that finally writes the
//! glyphs. Two would be two chances to disagree about how many lines a paragraph takes, and the disagreement
//! only ever shows up as a clipped last line in somebody else's terminal.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub const ELLIPSIS: &str = "…";

#[derive(Clone, Copy, Debug)]
pub struct WrapConfig {
    /// The widest a line may be, in cells. `0` is treated as `1`: a column that cannot hold one cell can
    /// hold nothing, and returning no lines at all loses the text instead of clipping it.
    pub max_cols: u16,
    pub wrap: bool,
    pub max_lines: Option<u16>,
    pub ellipsis: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WrappedLine {
    /// Byte range into the original string.
    pub range: Range<usize>,
    pub cols: u16,
    /// Whether an ellipsis is appended when this line is drawn, and its `range` cut to make room.
    pub ellipsized: bool,
}

/// The width of one grapheme cluster in cells. Zero-width clusters (combining marks alone) count as one so
/// they cannot make a line of infinite length.
pub fn grapheme_cols(g: &str) -> u16 {
    g.width().max(1) as u16
}

pub fn line_cols(s: &str) -> u16 {
    s.graphemes(true).map(grapheme_cols).sum()
}

pub fn wrap(text: &str, cfg: &WrapConfig, out: &mut Vec<WrappedLine>) {
    out.clear();
    let max_cols = cfg.max_cols.max(1);
    let mut hard_start = 0usize;
    for hard in text.split_inclusive('\n') {
        let trimmed = hard.strip_suffix('\n').unwrap_or(hard);
        let end = hard_start + trimmed.len();
        if cfg.wrap {
            wrap_one(text, hard_start..end, max_cols, out);
        } else {
            out.push(WrappedLine {
                range: hard_start..end,
                cols: line_cols(trimmed),
                ellipsized: false,
            });
        }
        hard_start += hard.len();
    }
    if out.is_empty() {
        out.push(WrappedLine {
            range: 0..0,
            cols: 0,
            ellipsized: false,
        });
    }
    clamp(text, cfg, max_cols, out);
}

/// Greedy word wrap over one hard-broken line. A word wider than the whole column breaks mid-word rather
/// than overflowing, because the alternative is a line the terminal truncates without telling anyone.
fn wrap_one(text: &str, span: Range<usize>, max_cols: u16, out: &mut Vec<WrappedLine>) {
    let slice = &text[span.clone()];
    if slice.is_empty() {
        out.push(WrappedLine {
            range: span,
            cols: 0,
            ellipsized: false,
        });
        return;
    }

    let base = span.start;
    let mut line_start = base;
    let mut line_cols = 0u16;
    // The width is carried with the position because the line up to a space is what the break keeps.
    let mut last_space: Option<(usize, u16)> = None;

    for (offset, g) in slice.grapheme_indices(true) {
        let at = base + offset;
        let w = grapheme_cols(g);
        let is_space = g.chars().all(char::is_whitespace);

        if line_cols + w > max_cols && at > line_start {
            let (cut, cols) = match last_space {
                // Break at the last space; the space itself belongs to neither line.
                Some((space_at, cols)) if space_at > line_start => (space_at, cols),
                // No space to break at: the word is wider than the column, so cut it here.
                _ => (at, line_cols),
            };
            out.push(WrappedLine {
                range: line_start..cut,
                cols,
                ellipsized: false,
            });
            line_start = skip_spaces(text, cut, span.end);
            line_cols = measure_between(text, line_start, at);
            last_space = None;
        }

        if is_space {
            last_space = Some((at, line_cols));
        }
        line_cols += w;
    }

    out.push(WrappedLine {
        range: line_start..span.end,
        cols: line_cols,
        ellipsized: false,
    });
}

fn skip_spaces(text: &str, from: usize, end: usize) -> usize {
    let mut at = from;
    for (offset, g) in text[from..end].grapheme_indices(true) {
        if !g.chars().all(char::is_whitespace) {
            return from + offset;
        }
        at = from + offset + g.len();
    }
    at
}

fn measure_between(text: &str, from: usize, to: usize) -> u16 {
    if from >= to {
        return 0;
    }
    line_cols(&text[from..to])
}

/// Applies `max_lines`, cutting the last kept line to make room for an ellipsis when one was asked for.
fn clamp(text: &str, cfg: &WrapConfig, max_cols: u16, out: &mut Vec<WrappedLine>) {
    let Some(max_lines) = cfg.max_lines.filter(|n| (out.len() as u16) > *n) else {
        return;
    };
    let max_lines = max_lines.max(1) as usize;
    out.truncate(max_lines);
    if !cfg.ellipsis {
        return;
    }
    let last = out.last_mut().expect("truncate keeps at least one line");
    let budget = max_cols.saturating_sub(line_cols(ELLIPSIS));
    let (range, cols) = cut_to(text, last.range.clone(), budget);
    last.range = range;
    last.cols = cols + line_cols(ELLIPSIS);
    last.ellipsized = true;
}

/// The longest prefix of `range` that fits in `budget` cells, on a grapheme boundary.
fn cut_to(text: &str, range: Range<usize>, budget: u16) -> (Range<usize>, u16) {
    let mut cols = 0u16;
    let mut end = range.start;
    for (offset, g) in text[range.clone()].grapheme_indices(true) {
        let w = grapheme_cols(g);
        if cols + w > budget {
            break;
        }
        cols += w;
        end = range.start + offset + g.len();
    }
    (range.start..end, cols)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(max_cols: u16) -> WrapConfig {
        WrapConfig {
            max_cols,
            wrap: true,
            max_lines: None,
            ellipsis: false,
        }
    }

    fn lines<'a>(text: &'a str, out: &[WrappedLine]) -> Vec<&'a str> {
        out.iter().map(|l| &text[l.range.clone()]).collect()
    }

    #[test]
    fn breaks_at_spaces() {
        let text = "the quick brown fox";
        let mut out = Vec::new();
        wrap(text, &cfg(10), &mut out);
        assert_eq!(lines(text, &out), vec!["the quick", "brown fox"]);
    }

    #[test]
    fn a_word_wider_than_the_column_is_cut() {
        let text = "abcdefghijkl";
        let mut out = Vec::new();
        wrap(text, &cfg(5), &mut out);
        assert_eq!(lines(text, &out), vec!["abcde", "fghij", "kl"]);
    }

    #[test]
    fn hard_breaks_are_kept() {
        let text = "a\nb";
        let mut out = Vec::new();
        wrap(text, &cfg(80), &mut out);
        assert_eq!(lines(text, &out), vec!["a", "b"]);
    }

    #[test]
    fn no_wrap_keeps_one_line_per_hard_break() {
        let text = "a very long line indeed";
        let mut out = Vec::new();
        wrap(
            text,
            &WrapConfig {
                wrap: false,
                ..cfg(5)
            },
            &mut out,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].cols, 23);
    }

    #[test]
    fn wide_glyphs_count_two_cells() {
        let text = "漢字漢字";
        let mut out = Vec::new();
        wrap(text, &cfg(4), &mut out);
        assert_eq!(lines(text, &out), vec!["漢字", "漢字"]);
    }

    #[test]
    fn clamping_appends_an_ellipsis_within_the_column() {
        let text = "the quick brown fox";
        let mut out = Vec::new();
        wrap(
            text,
            &WrapConfig {
                max_lines: Some(1),
                ellipsis: true,
                ..cfg(10)
            },
            &mut out,
        );
        assert_eq!(out.len(), 1);
        assert!(out[0].ellipsized);
        assert!(out[0].cols <= 10, "got {}", out[0].cols);
    }

    #[test]
    fn clamping_without_an_ellipsis_just_drops_lines() {
        let text = "one two three four";
        let mut out = Vec::new();
        wrap(
            text,
            &WrapConfig {
                max_lines: Some(2),
                ..cfg(7)
            },
            &mut out,
        );
        assert_eq!(out.len(), 2);
        assert!(!out[1].ellipsized);
    }

    #[test]
    fn empty_text_is_one_empty_line() {
        let mut out = Vec::new();
        wrap("", &cfg(10), &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].cols, 0);
    }
}
