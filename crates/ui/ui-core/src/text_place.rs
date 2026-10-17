//! Where an editor draws its text: the rows it is broken into, and the x of every offset along them, which is where the caret, the selection, the underlines and a press all meet the letters.

use std::cell::{Cell, OnceCell, RefCell};
use std::ops::Range;
use std::rc::Rc;

use geometry_core::Rect;
use layout_core::Direction;
use renderer_core::{TextAlign, TextStyle, TextWrap};

use crate::text_edit::next_boundary;
use crate::visual_lines::{Affinity, VisualLines};

/// Width of the caret, in logical px.
pub(crate) const CARET_WIDTH: f32 = 1.5;

/// How wide a row is measured as when nothing should stop it.
const UNBOUNDED: f32 = 1.0e6;

/// The rows of one text at one width, kept until the text, the width, the style or the fonts move.
struct Laid {
    text: String,
    width: Option<f32>,
    style: TextStyle,
    mask: Option<char>,
    generation: u64,
    lines: Rc<VisualLines>,
    widest: Rc<OnceCell<f32>>,
}

/// The last text an editor broke into rows, so a frame that only moved the caret measures nothing again.
#[derive(Default)]
pub(crate) struct RowCache(RefCell<Option<Laid>>);

impl RowCache {
    fn laid(
        &self,
        text: &str,
        style: &TextStyle,
        width: Option<f32>,
        mask: Option<char>,
    ) -> (Rc<VisualLines>, Rc<OnceCell<f32>>) {
        let generation = renderer_core::text_metrics_generation();
        if let Some(laid) = self.0.borrow().as_ref()
            && laid.text == text
            && laid.width == width
            && laid.mask == mask
            && laid.generation == generation
            && laid.style.same_extent(style)
        {
            return (Rc::clone(&laid.lines), Rc::clone(&laid.widest));
        }
        let lines = Rc::new(VisualLines::of(text, style, width));
        let widest = Rc::new(OnceCell::new());
        *self.0.borrow_mut() = Some(Laid {
            text: text.to_string(),
            width,
            style: style.clone(),
            mask,
            generation,
            lines: Rc::clone(&lines),
            widest: Rc::clone(&widest),
        });
        (lines, widest)
    }

    pub(crate) fn lines(
        &self,
        text: &str,
        style: &TextStyle,
        width: Option<f32>,
    ) -> Rc<VisualLines> {
        self.laid(text, style, width, None).0
    }
}

/// How an editor's box frames its text.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Frame {
    /// The width rows break at, `None` when only a `\n` breaks them.
    pub wrap: Option<f32>,
    /// The box's width.
    pub width: f32,
    /// How far down the first row starts.
    pub top: f32,
    /// How far the text is scrolled sideways.
    pub scroll_x: f32,
    pub line_h: f32,
}

/// Where a press lands in the text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Hit {
    /// The offset whose caret is nearest the press.
    pub at: usize,
    /// The row that caret stands on, where the end of a wrapped row is also the start of the next.
    pub affinity: Affinity,
    /// The start of the character under the press, which is what a double-click selects the word of.
    pub under: usize,
}

/// One text laid out in an editor's box, in the box's own coordinates.
pub(crate) struct Placed<'a> {
    pub text: &'a str,
    pub lines: Rc<VisualLines>,
    flat: TextStyle,
    mask: Option<char>,
    align: TextAlign,
    pub frame: Frame,
    widest: Rc<OnceCell<f32>>,
    row_origins: RefCell<Vec<Option<f32>>>,
    starts_flush: Cell<Option<bool>>,
}

impl<'a> Placed<'a> {
    /// `text` in the rows `rows` breaks it into at `frame`, drawn in `style` with each character shown as `mask` where there is one.
    pub(crate) fn new(
        rows: &RowCache,
        text: &'a str,
        style: &TextStyle,
        mask: Option<char>,
        frame: Frame,
    ) -> Self {
        let (lines, widest) = rows.laid(text, style, frame.wrap, mask);
        let flat = style.clone().with_text_wrap(TextWrap::NoWrap);
        let count = lines.len();
        Self {
            text,
            lines,
            flat,
            mask,
            align: style.text_align,
            frame,
            widest,
            row_origins: RefCell::new(vec![None; count]),
            starts_flush: Cell::new(None),
        }
    }

    /// How wide `text[range]` is drawn: the letters, or a mask character for each of them.
    pub(crate) fn advance(&self, range: Range<usize>) -> f32 {
        let run = &self.text[range];
        if run.is_empty() {
            return 0.0;
        }
        let measure =
            |run: &str| crate::text_metrics::measure_text(run, None, UNBOUNDED, &self.flat).0;
        match self.mask {
            Some(bullet) => measure(&run.chars().map(|_| bullet).collect::<String>()),
            None => measure(run),
        }
    }

    /// How wide the widest row is drawn.
    pub(crate) fn widest(&self) -> f32 {
        *self.widest.get_or_init(|| {
            (0..self.lines.len())
                .map(|row| self.row_width(row))
                .fold(0.0, f32::max)
        })
    }

    /// The width of the rect the text is drawn in, which each row is aligned within: the width rows wrap at, or wide enough for the widest of them.
    pub(crate) fn text_width(&self) -> f32 {
        match self.frame.wrap {
            Some(width) => width,
            None => self.frame.width.max(self.widest()),
        }
    }

    /// How far the text runs past the box, which is as far as it can be scrolled sideways: the widest row and the caret after it.
    pub(crate) fn max_scroll_x(&self) -> f32 {
        match self.frame.wrap {
            Some(_) => 0.0,
            None => (self.widest() + CARET_WIDTH - self.frame.width).max(0.0),
        }
    }

    /// A row's drawn width, less the space a wrapped row broke on, which hangs past the edge.
    fn row_width(&self, row: usize) -> f32 {
        let range = self.lines.row(row);
        let end = match self.lines.wraps_after(row) {
            true => range.start + self.text[range.clone()].trim_end().len(),
            false => range.end,
        };
        self.advance(range.start..end)
    }

    fn flush_start(&self) -> bool {
        if let Some(known) = self.starts_flush.get() {
            return known;
        }
        let flush = matches!(self.align, TextAlign::Start | TextAlign::Justify)
            && layout_reactive::current_direction() == Direction::Ltr;
        self.starts_flush.set(Some(flush));
        flush
    }

    /// Where the first glyph of `row` sits, before any scroll: where the text's alignment puts a row that wide in the rect the text is drawn in.
    fn origin(&self, row: usize) -> f32 {
        if self.flush_start() {
            return 0.0;
        }
        let row = row.min(self.lines.len() - 1);
        if let Some(known) = self.row_origins.borrow()[row] {
            return known;
        }
        let origin = crate::caret::align_origin(self.align, self.text_width(), self.row_width(row));
        self.row_origins.borrow_mut()[row] = Some(origin);
        origin
    }

    /// The x of byte `at` on `row` before any scroll, held inside the box a wrapped row is drawn in, which the space a row broke on hangs past.
    pub(crate) fn content_x(&self, row: usize, at: usize) -> f32 {
        let range = self.lines.row(row);
        let at = at.clamp(range.start, self.text.len());
        let x = self.origin(row) + self.advance(range.start..at);
        match self.frame.wrap {
            Some(width) => x.min((width - CARET_WIDTH).max(0.0)),
            None => x,
        }
    }

    /// The x of byte `at` on `row`, in the box.
    pub(crate) fn x_at(&self, row: usize, at: usize) -> f32 {
        self.content_x(row, at) - self.frame.scroll_x
    }

    pub(crate) fn row_of(&self, at: usize, affinity: Affinity) -> usize {
        self.lines.row_of(at, affinity)
    }

    pub(crate) fn row_top(&self, row: usize) -> f32 {
        self.frame.top + row as f32 * self.frame.line_h
    }

    /// The caret at byte `at`, on the row `affinity` puts it on.
    pub(crate) fn caret_rect(&self, at: usize, affinity: Affinity) -> Rect {
        let row = self.row_of(at, affinity);
        Rect {
            x: self.x_at(row, at),
            y: self.row_top(row),
            width: CARET_WIDTH,
            height: self.frame.line_h,
        }
    }

    /// The row a press at `y` in the box is on, the first or the last for one above or below the text.
    pub(crate) fn row_at(&self, y: f32) -> usize {
        let row = ((y - self.frame.top) / self.frame.line_h).floor();
        (row.max(0.0) as usize).min(self.lines.len() - 1)
    }

    /// Where a press at `(x, y)` in the box lands.
    pub(crate) fn hit(&self, x: f32, y: f32) -> Hit {
        self.hit_row(self.row_at(y), x)
    }

    /// Where a press at `x` on `row` lands. The caret's x only grows along a row, so it is searched for rather than walked to.
    pub(crate) fn hit_row(&self, row: usize, x: f32) -> Hit {
        let range = self.lines.row(row);
        let mut stops = vec![range.start];
        let mut at = range.start;
        while at < range.end {
            at = next_boundary(self.text, at);
            stops.push(at);
        }
        let after = stops.partition_point(|stop| self.x_at(row, *stop) <= x);
        let before = after.saturating_sub(1);
        let nearest = match stops.get(after) {
            Some(next) if self.x_at(row, *next) - x < x - self.x_at(row, stops[before]) => *next,
            _ => stops[before],
        };
        let under = stops[before.min(stops.len().saturating_sub(2))];
        let affinity = match nearest == range.end && self.lines.wraps_after(row) {
            true => Affinity::Upstream,
            false => Affinity::Downstream,
        };
        Hit {
            at: nearest,
            affinity,
            under,
        }
    }

    /// The stretch of each row `span` covers, as `(row, x0, x1)` in the box. A row of no characters the span still crosses is given a sliver, so a selection reads as continuous through it.
    pub(crate) fn bands(&self, span: Range<usize>, sliver: bool) -> Vec<(usize, f32, f32)> {
        let first = self.row_of(span.start, Affinity::Downstream);
        let last = self.row_of(span.end, Affinity::Upstream).max(first);
        (first..=last)
            .filter_map(|row| {
                let range = self.lines.row(row);
                let (from, to) = (span.start.max(range.start), span.end.min(range.end));
                if from > to {
                    return None;
                }
                let x0 = self.x_at(row, from);
                let mut x1 = self.x_at(row, to);
                let runs_on = row < last && !self.lines.wraps_after(row);
                if sliver && runs_on {
                    x1 = x1.max(x0 + self.frame.line_h * 0.35);
                }
                Some((row, x0, x1))
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "text_place_test.rs"]
mod tests;
