//! Paragraphs, in the glyphs themselves.

use geometry_core::Rect;
use renderer_core::{Declared, FontStyle, Paint, Span, TextAlign, TextStyle};
use unicode_segmentation::UnicodeSegmentation;

use super::Painter;
use super::geom::{cell_center, mapped, sample};
use crate::cell::{Attrs, Grapheme};
use crate::wrap::{ELLIPSIS, grapheme_cols};

impl Painter<'_> {
    pub(crate) fn text(
        &mut self,
        text: &str,
        spans: Option<&[Span]>,
        rect: Rect,
        style: &TextStyle,
    ) {
        let cells = self.cells_of_unclipped(rect);
        if cells.is_empty() {
            return;
        }
        let cols = cells.cols();

        // The painter re-wraps rather than trusting what layout measured, at the cell width of the box it actually got. The two can differ by a column when a box was sized in pixels, and a paragraph laid out one column wider than it is drawn loses its last word.
        let mut lines = std::mem::take(&mut self.lines);
        self.metrics
            .lines(text, cols as f32 * self.cell.width, style, &mut lines);

        let mut resolved = SpanCursor::new(spans, style);
        for (i, line) in lines.iter().enumerate() {
            let row = cells.row0 + i as i32;
            if row >= cells.row1 {
                break;
            }
            let width = line.cols
                + if line.ellipsized {
                    grapheme_cols(ELLIPSIS)
                } else {
                    0
                };
            let mut col = cells.col0
                + match style.text_align {
                    TextAlign::Center => (cols.saturating_sub(width) / 2) as i32,
                    TextAlign::End => cols.saturating_sub(width) as i32,
                    TextAlign::Start | TextAlign::Justify => 0,
                };
            for (offset, grapheme) in text[line.range.clone()].grapheme_indices(true) {
                if col >= cells.col1 {
                    break;
                }
                let at = line.range.start + offset;
                let (paint, attrs) = resolved.at(at);
                let advance = self.draw_grapheme(col, row, grapheme, &paint, attrs);
                col += advance as i32;
            }
            if line.ellipsized && col < cells.col1 {
                let (paint, attrs) = resolved.at(line.range.end.saturating_sub(1));
                self.draw_grapheme(col, row, ELLIPSIS, &paint, attrs);
            }
        }

        lines.clear();
        self.lines = lines;
    }

    fn draw_grapheme(
        &mut self,
        col: i32,
        row: i32,
        grapheme: &str,
        paint: &Paint,
        attrs: Attrs,
    ) -> u16 {
        let width = grapheme_cols(grapheme);
        if !self.clipped_in(col, row) {
            return width;
        }
        let mapped = mapped(paint, self.matrix(), self.scale());
        let p = cell_center(col, row, self.cell);
        let color = sample(&mapped, p.x, p.y);
        self.put_glyph(col, row, Grapheme::new(grapheme), color, attrs)
    }

    /// Cells for a rect, mapped but *not* cut to the clip: alignment is measured against the box the text was given, not against the part of it that happens to be visible. Individual cells are clip-tested as they are written.
    fn cells_of_unclipped(&self, rect: Rect) -> super::CellRect {
        let mapped = renderer_core::transform_clip_rect(self.matrix(), rect);
        super::CellRect::of(mapped, self.cell)
    }
}

fn attrs_of(style: &TextStyle) -> Attrs {
    let mut attrs = Attrs::NONE;
    if style.font_weight >= 600 {
        attrs = attrs.with(Attrs::BOLD);
    }
    if style.font_style != FontStyle::Normal {
        attrs = attrs.with(Attrs::ITALIC);
    }
    // A terminal has one face, so a weight below normal is the only "lighter" it can offer.
    if style.font_weight <= 300 {
        attrs = attrs.with(Attrs::DIM);
    }
    attrs
}

/// Walks a paragraph's span overrides in step with the graphemes being drawn, so each one costs a range check rather than a search.
struct SpanCursor<'a> {
    spans: &'a [Span],
    base: (Paint, Attrs),
    active: Option<(std::ops::Range<u32>, Paint, Attrs)>,
    next: usize,
    base_style: &'a TextStyle,
}

impl<'a> SpanCursor<'a> {
    fn new(spans: Option<&'a [Span]>, style: &'a TextStyle) -> Self {
        Self {
            spans: spans.unwrap_or(&[]),
            base: (style.color, attrs_of(style)),
            active: None,
            next: 0,
            base_style: style,
        }
    }

    fn at(&mut self, offset: usize) -> (Paint, Attrs) {
        if self.spans.is_empty() {
            return self.base;
        }
        let offset = offset as u32;
        if let Some((range, paint, attrs)) = &self.active {
            if range.contains(&offset) {
                return (*paint, *attrs);
            }
            self.active = None;
        }
        // Spans arrive in order, so the search only ever moves forwards across one paragraph.
        while self.next < self.spans.len() && self.spans[self.next].range.end <= offset {
            self.next += 1;
        }
        if let Some(span) = self
            .spans
            .get(self.next)
            .filter(|s| s.range.contains(&offset))
        {
            let style = resolve(&span.over, self.base_style);
            let entry = (span.range.clone(), style.color, attrs_of(&style));
            let out = (entry.1, entry.2);
            self.active = Some(entry);
            return out;
        }
        self.base
    }
}

fn resolve(declared: &Declared, base: &TextStyle) -> TextStyle {
    declared.clone().over(base)
}
