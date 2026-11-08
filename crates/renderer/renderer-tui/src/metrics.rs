//! What a string is worth to layout when a "pixel" is a fraction of a character cell.

use geometry_core::CellSize;
use renderer_core::{Span, TextAlign, TextLine, TextMetrics, TextStyle, TextWrap, case_text};
use unicode_segmentation::UnicodeSegmentation;

use crate::wrap::{WrapConfig, WrappedLine, grapheme_cols, line_cols, wrap};

/// The column a line `width` columns wide starts at in a box `cols` wide, as `align` places it.
pub(crate) fn align_cols(align: TextAlign, cols: u16, width: u16) -> u16 {
    match align {
        TextAlign::Center => cols.saturating_sub(width) / 2,
        TextAlign::End => cols.saturating_sub(width),
        TextAlign::Start | TextAlign::Justify => 0,
    }
}

/// Measures text in whole cells, so a box sized by its content always lands on a cell boundary.
#[derive(Clone, Copy, Debug)]
pub struct CellMetrics {
    cell: CellSize,
}

impl CellMetrics {
    pub fn new(cell: CellSize) -> Self {
        Self { cell }
    }

    fn config(&self, max_width: f32, style: &TextStyle) -> WrapConfig {
        WrapConfig {
            // Taffy probes intrinsic width with an effectively unbounded available space; a column count derived from it would overflow, and the answer wanted there is "however wide it wants to be".
            max_cols: if max_width.is_finite() && max_width < 1.0e5 {
                self.cell.cols_in(max_width).max(1)
            } else {
                u16::MAX
            },
            wrap: style.text_wrap == TextWrap::Wrap,
            max_lines: style.clamp.max_lines().map(|n| n as u16),
            ellipsis: style.clamp.ellipsis(),
        }
    }

    /// The wrapped lines of `text` under `style` at `max_width`. The painter calls this with the cell width of the box it actually got, so what it draws is what layout was told.
    pub fn lines(&self, text: &str, max_width: f32, style: &TextStyle, out: &mut Vec<WrappedLine>) {
        wrap(text, &self.config(max_width, style), out);
    }

    fn extent(&self, text: &str, max_width: f32, style: &TextStyle) -> (f32, f32) {
        let mut lines = Vec::new();
        self.lines(text, max_width, style, &mut lines);
        let cols = lines.iter().map(|l| l.cols).max().unwrap_or(0);
        (
            cols as f32 * self.cell.width,
            lines.len() as f32 * self.cell.height,
        )
    }

    fn ink(&self, text: &str, max_width: f32, style: &TextStyle) -> (f32, f32) {
        let mut lines = Vec::new();
        self.lines(text, max_width, style, &mut lines);
        if lines.iter().all(|line| line.cols == 0) {
            return (0.0, 0.0);
        }
        (0.0, lines.len() as f32 * self.cell.height)
    }
}

impl TextMetrics for CellMetrics {
    /// Spans change colour and weight, never advance: every cell is one column wide whatever face it claims to be in, which is the one thing a terminal is strict about.
    fn measure(
        &self,
        text: &str,
        spans: Option<&[Span]>,
        max_width: f32,
        style: &TextStyle,
    ) -> (f32, f32) {
        self.extent(&case_text(text, spans, style).text, max_width, style)
    }

    fn min_content(&self, text: &str, spans: Option<&[Span]>, style: &TextStyle) -> (f32, f32) {
        let cased = case_text(text, spans, style);
        let text = cased.text.as_ref();
        let widest = text.split_whitespace().map(line_cols).max().unwrap_or(0);
        self.extent(text, f32::from(widest) * self.cell.width, style)
    }

    fn ink_bounds(&self, text: &str, max_width: f32, style: &TextStyle) -> (f32, f32) {
        self.ink(&case_text(text, None, style).text, max_width, style)
    }

    fn line_height(&self, _font_size: f32) -> f32 {
        self.cell.height
    }

    /// The painter's own wrap and alignment, without the clamp: a terminal draws the cells in the order they are written, each grapheme cluster as many columns wide as it takes.
    fn laid_lines(
        &self,
        text: &str,
        spans: Option<&[Span]>,
        max_width: f32,
        style: &TextStyle,
    ) -> Vec<TextLine> {
        let cased = case_text(text, spans, style);
        let config = WrapConfig {
            max_lines: None,
            ..self.config(max_width, style)
        };
        let mut lines = Vec::new();
        wrap(&cased.text, &config, &mut lines);
        if cased.text.ends_with('\n') {
            let end = cased.text.len();
            lines.push(WrappedLine {
                range: end..end,
                cols: 0,
                ellipsized: false,
            });
        }
        let cols = self.cell.cols_in(max_width);
        let cell = self.cell.width;
        lines
            .into_iter()
            .map(|line| {
                let start = align_cols(style.text_align, cols, line.cols);
                TextLine::left_to_right(&cased, text, line.range, f32::from(start) * cell, |part| {
                    f32::from(grapheme_cols(part)) * cell
                })
            })
            .collect()
    }

    /// The cell under the point, found the way the painter lays the paragraph out: the same wrap, the same alignment inside the columns it was given.
    fn index_at(
        &self,
        text: &str,
        spans: Option<&[Span]>,
        max_width: f32,
        style: &TextStyle,
        (x, y): (f32, f32),
    ) -> Option<usize> {
        if x < 0.0 || y < 0.0 {
            return None;
        }
        let cased = case_text(text, spans, style);
        let text = cased.text.as_ref();
        let mut lines = Vec::new();
        self.lines(text, max_width, style, &mut lines);
        let line = lines.get((y / self.cell.height) as usize)?;
        let start = align_cols(style.text_align, self.cell.cols_in(max_width), line.cols);
        let target = (x / self.cell.width) as u16;
        let mut col = start;
        for (offset, grapheme) in text[line.range.clone()].grapheme_indices(true) {
            let width = grapheme_cols(grapheme);
            if (col..col + width).contains(&target) {
                return Some(cased.source_index(line.range.start + offset));
            }
            col += width;
        }
        None
    }

    /// The painter's own wrap, without the clamp: where each line it would draw begins.
    fn line_starts(
        &self,
        text: &str,
        spans: Option<&[Span]>,
        max_width: f32,
        style: &TextStyle,
    ) -> Vec<usize> {
        let cased = case_text(text, spans, style);
        let config = WrapConfig {
            max_lines: None,
            ..self.config(max_width, style)
        };
        let mut lines = Vec::new();
        wrap(&cased.text, &config, &mut lines);
        let mut starts: Vec<usize> = lines
            .iter()
            .map(|line| cased.source_index(line.range.start))
            .collect();
        starts.dedup();
        starts
    }
}

#[cfg(test)]
#[path = "metrics.test.rs"]
mod tests;
