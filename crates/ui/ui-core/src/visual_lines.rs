//! [`VisualLines`]: the rows a multi-line text is drawn in once it wraps, which is what a caret moves along.

use std::ops::Range;

use renderer_core::{TextLine, TextStyle};

/// Which row a caret at a soft wrap stands on: the offset where a wrapped row ends is the one where the next begins, so the offset alone cannot say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Affinity {
    /// At the start of the later row: where typing and every horizontal move leave the caret.
    Downstream,
    /// At the end of the earlier row: where End and a press past the end of a row put it.
    Upstream,
}

/// The byte range each row of a text holds, in order. A row a `\n` ends stops before it, and a row the width ends stops where the next one starts.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VisualLines {
    rows: Vec<Range<usize>>,
}

impl VisualLines {
    /// `text` broken at every `\n` and, given a width, wherever the installed measurer wraps it at that width.
    pub(crate) fn of(text: &str, style: &TextStyle, wrap_width: Option<f32>) -> Self {
        match wrap_width {
            Some(width) => Self::drawn(text, &crate::text_metrics::laid_lines(text, width, style)),
            None => Self::unwrapped(text),
        }
    }

    /// `text` broken only at its `\n`s.
    pub(crate) fn unwrapped(text: &str) -> Self {
        Self::broken(text, std::iter::empty())
    }

    /// `text` broken at every `\n` and wherever one of `lines`, the lines it is drawn in, begins: the rows are the lines on screen, so a caret placed on a row is placed on the line drawn there.
    pub(crate) fn drawn(text: &str, lines: &[TextLine]) -> Self {
        Self::broken(text, lines.iter().map(|line| line.range.start))
    }

    /// `text` broken at every `\n` and at each of `starts`, which ascend; a start a row already begins at breaks nothing.
    fn broken(text: &str, starts: impl Iterator<Item = usize>) -> Self {
        let mut starts = starts.peekable();
        let mut rows = Vec::new();
        let mut start = 0;
        for hard in text.split('\n') {
            let end = start + hard.len();
            let mut from = start;
            while let Some(at) = starts.next_if(|&at| at <= end) {
                if at > from && at < end {
                    rows.push(from..at);
                    from = at;
                }
            }
            rows.push(from..end);
            start = end + 1;
        }
        Self { rows }
    }

    pub(crate) fn len(&self) -> usize {
        self.rows.len()
    }

    /// Row `index`, or the last row past the end.
    pub(crate) fn row(&self, index: usize) -> Range<usize> {
        self.rows[index.min(self.rows.len() - 1)].clone()
    }

    /// Whether row `index` is ended by the width rather than by a `\n`.
    pub(crate) fn wraps_after(&self, index: usize) -> bool {
        self.rows
            .get(index + 1)
            .is_some_and(|next| next.start == self.rows[index].end)
    }

    /// The row a caret at byte `at` is drawn on.
    pub(crate) fn row_of(&self, at: usize, affinity: Affinity) -> usize {
        let row = self
            .rows
            .partition_point(|row| row.start <= at)
            .saturating_sub(1);
        let ends_the_row_before =
            row > 0 && self.rows[row].start == at && self.wraps_after(row - 1);
        if affinity == Affinity::Upstream && ends_the_row_before {
            row - 1
        } else {
            row
        }
    }
}

#[cfg(test)]
#[path = "visual_lines.test.rs"]
mod tests;
