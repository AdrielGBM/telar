//! What a string is worth to layout when a "pixel" is a fraction of a character cell.

use renderer_core::{Span, TextMetrics, TextStyle, TextWrap};

use crate::wrap::{WrapConfig, WrappedLine, wrap};

/// How many logical pixels one terminal cell stands for.
///
/// The terminal has no pixels, but every layout value in a Telar app is written in them — `pad:24`, `width:300`, a theme's `gutter`. Rather than teach the layout engine a second unit, the terminal declares an exchange rate and reports its own size in the same currency: an 80×24 terminal at the default rate is a 640×384 "window". Everything above stays exactly as it is on the desktop, and the rounding to whole cells happens once, in the painter, on absolute edges.
///
/// The default is the proportion of a typical monospace face — twice as tall as it is wide — which is what makes a layout authored for the desktop come out with recognisable proportions here.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellSize {
    pub width: f32,
    pub height: f32,
}

impl Default for CellSize {
    fn default() -> Self {
        Self {
            width: 8.0,
            height: 16.0,
        }
    }
}

impl CellSize {
    /// The column a logical x falls in. Rounds, and callers round *both* edges of a box rather than its width, so two boxes that touch in logical space still touch in cells.
    pub fn col_at(self, x: f32) -> i32 {
        (x / self.width).round() as i32
    }

    pub fn row_at(self, y: f32) -> i32 {
        (y / self.height).round() as i32
    }

    pub fn cols_in(self, width: f32) -> u16 {
        (width / self.width).floor().clamp(0.0, u16::MAX as f32) as u16
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
}

impl TextMetrics for CellMetrics {
    /// Spans change colour and weight, never advance: every cell is one column wide whatever face it claims to be in, which is the one thing a terminal is strict about.
    fn measure(
        &self,
        text: &str,
        _spans: Option<&[Span]>,
        max_width: f32,
        style: &TextStyle,
    ) -> (f32, f32) {
        self.extent(text, max_width, style)
    }

    fn ink_bounds(&self, text: &str, max_width: f32, style: &TextStyle) -> (f32, f32) {
        self.extent(text, max_width, style)
    }

    fn line_height(&self, _font_size: f32) -> f32 {
        self.cell.height
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use renderer_core::{Clamp, Color};

    fn style() -> TextStyle {
        TextStyle::new(14.0, renderer_core::Paint::Solid(Color::WHITE))
    }

    fn metrics() -> CellMetrics {
        CellMetrics::new(CellSize::default())
    }

    #[test]
    fn a_measured_box_lands_on_cell_boundaries() {
        let (w, h) = metrics().measure("hello", None, 1000.0, &style());
        assert_eq!(w, 5.0 * 8.0);
        assert_eq!(h, 16.0);
    }

    #[test]
    fn wrapping_reports_the_extra_lines() {
        let (_, h) = metrics().measure("the quick brown fox", None, 10.0 * 8.0, &style());
        assert_eq!(h, 2.0 * 16.0);
    }

    #[test]
    fn an_unbounded_probe_does_not_wrap() {
        let (w, h) = metrics().measure("the quick brown fox", None, 1.0e6, &style());
        assert_eq!(h, 16.0);
        assert_eq!(w, 19.0 * 8.0);
    }

    #[test]
    fn a_clamp_caps_the_height() {
        let mut s = style();
        s.clamp = Clamp::lines(1, true);
        let (_, h) = metrics().measure("the quick brown fox", None, 10.0 * 8.0, &s);
        assert_eq!(h, 16.0);
    }

    #[test]
    fn line_height_ignores_font_size() {
        assert_eq!(metrics().line_height(48.0), 16.0);
    }

    #[test]
    fn edges_round_so_neighbours_touch() {
        let cell = CellSize::default();
        let first = (cell.col_at(0.0), cell.col_at(37.5));
        let second = (cell.col_at(37.5), cell.col_at(75.0));
        assert_eq!(first.1, second.0, "a shared edge must land on one column");
    }
}
