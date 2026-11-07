//! Where an editor draws its text: the rows it is broken into, and the x of every offset along them, which is where the caret, the selection, the underlines and a press all meet the letters.
//!
//! Every x is read off the lines the installed measurer lays the text out in, cluster by cluster, so a right-to-left run is placed where it is drawn rather than where its bytes fall.

use std::cell::{OnceCell, RefCell};
use std::ops::Range;
use std::rc::Rc;

use geometry_core::Rect;
use renderer_core::{TextCluster, TextLine, TextStyle, TextWrap};
use unicode_segmentation::UnicodeSegmentation;

use crate::visual_lines::{Affinity, VisualLines};

/// Width of the caret, in logical px.
pub(crate) const CARET_WIDTH: f32 = 1.5;

/// How wide a row is measured as when nothing should stop it.
const UNBOUNDED: f32 = 1.0e6;

/// The lines of a text as drawn at one width, cut into clusters.
type Drawn = Rc<RefCell<Option<(f32, Rc<[TextLine]>)>>>;

/// The rows of one text at one width, kept until the text, the width, the style or the fonts move.
struct Laid {
    text: String,
    width: Option<f32>,
    style: TextStyle,
    mask: Option<char>,
    generation: u64,
    lines: Rc<VisualLines>,
    widest: Rc<OnceCell<f32>>,
    drawn: Drawn,
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
    ) -> (Rc<VisualLines>, Rc<OnceCell<f32>>, Drawn) {
        let generation = renderer_core::text_metrics_generation();
        if let Some(laid) = self.0.borrow().as_ref()
            && laid.text == text
            && laid.width == width
            && laid.mask == mask
            && laid.generation == generation
            && laid.style.same_extent(style)
        {
            return (
                Rc::clone(&laid.lines),
                Rc::clone(&laid.widest),
                Rc::clone(&laid.drawn),
            );
        }
        let lines = Rc::new(VisualLines::of(text, style, width));
        let widest = Rc::new(OnceCell::new());
        let drawn = Drawn::default();
        *self.0.borrow_mut() = Some(Laid {
            text: text.to_string(),
            width,
            style: style.clone(),
            mask,
            generation,
            lines: Rc::clone(&lines),
            widest: Rc::clone(&widest),
            drawn: Rc::clone(&drawn),
        });
        (lines, widest, drawn)
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

/// `text` as a masked editor draws it: one `bullet` for every grapheme cluster, so a hidden flag or accented letter is one bullet, as it is one press of Backspace.
pub(crate) fn masked(text: &str, bullet: char) -> String {
    text.graphemes(true).map(|_| bullet).collect()
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
    /// The start of the cluster under the press, which is what a double-click selects the word of.
    pub under: usize,
}

/// One text laid out in an editor's box, in the box's own coordinates.
pub(crate) struct Placed<'a> {
    pub text: &'a str,
    pub lines: Rc<VisualLines>,
    style: TextStyle,
    flat: TextStyle,
    mask: Option<char>,
    pub frame: Frame,
    widest: Rc<OnceCell<f32>>,
    drawn: Drawn,
}

impl<'a> Placed<'a> {
    /// `text` in the rows `rows` breaks it into at `frame`, drawn in `style` with each grapheme cluster shown as `mask` where there is one.
    pub(crate) fn new(
        rows: &RowCache,
        text: &'a str,
        style: &TextStyle,
        mask: Option<char>,
        frame: Frame,
    ) -> Self {
        let (lines, widest, drawn) = rows.laid(text, style, frame.wrap, mask);
        Self {
            text,
            lines,
            style: style.clone(),
            flat: style.clone().with_text_wrap(TextWrap::NoWrap),
            mask,
            frame,
            widest,
            drawn,
        }
    }

    /// How wide `text[range]` is drawn: the letters, or a mask character for each cluster of them.
    pub(crate) fn advance(&self, range: Range<usize>) -> f32 {
        let run = &self.text[range];
        if run.is_empty() {
            return 0.0;
        }
        let measure =
            |run: &str| crate::text_metrics::measure_text(run, None, UNBOUNDED, &self.flat).0;
        match self.mask {
            Some(bullet) => measure(&masked(run, bullet)),
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

    /// The lines the text is drawn in, as wide as [`text_width`](Self::text_width), laid out once per text and width.
    fn drawn(&self) -> Rc<[TextLine]> {
        let width = self.text_width();
        if let Some((at, lines)) = self.drawn.borrow().as_ref()
            && *at == width
        {
            return Rc::clone(lines);
        }
        let lines: Rc<[TextLine]> = self.lay_out(width).into();
        *self.drawn.borrow_mut() = Some((width, Rc::clone(&lines)));
        lines
    }

    fn lay_out(&self, width: f32) -> Vec<TextLine> {
        let Some(bullet) = self.mask else {
            return crate::text_metrics::laid_lines(self.text, width, &self.style);
        };
        let bounds: Vec<usize> = self
            .text
            .grapheme_indices(true)
            .map(|(at, _)| at)
            .chain(std::iter::once(self.text.len()))
            .collect();
        let step = bullet.len_utf8();
        let written = |shown: usize| bounds[(shown / step).min(bounds.len() - 1)];
        crate::text_metrics::laid_lines(&masked(self.text, bullet), width, &self.style)
            .into_iter()
            .map(|mut line| {
                line.range = written(line.range.start)..written(line.range.end);
                for cluster in &mut line.clusters {
                    cluster.bytes = written(cluster.bytes.start)..written(cluster.bytes.end);
                }
                line
            })
            .collect()
    }

    /// The x of byte `at` on `row` before any scroll, held inside the box a wrapped row is drawn in, which the space a row broke on hangs past.
    pub(crate) fn content_x(&self, row: usize, at: usize) -> f32 {
        let drawn = self.drawn();
        let x = line_of(&drawn, &self.lines, row).map_or(0.0, |line| caret_on(line, at));
        self.held_in(x)
    }

    fn held_in(&self, x: f32) -> f32 {
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

    /// Where a press at `x` on `row` lands: on the side of the cluster nearest it that the press is on, which in a right-to-left cluster is the side its bytes end on.
    pub(crate) fn hit_row(&self, row: usize, x: f32) -> Hit {
        let range = self.lines.row(row);
        let x = x + self.frame.scroll_x;
        let drawn = self.drawn();
        let clusters: Vec<&TextCluster> = line_of(&drawn, &self.lines, row)
            .map(|line| {
                line.clusters
                    .iter()
                    .filter(|cluster| {
                        cluster.bytes.start >= range.start && cluster.bytes.end <= range.end
                    })
                    .collect()
            })
            .unwrap_or_default();
        let nearest = clusters
            .iter()
            .min_by(|a, b| distance(a, x).total_cmp(&distance(b, x)));
        let (at, under) = match nearest {
            Some(cluster) => {
                let on_left = x < cluster.x + cluster.width / 2.0;
                let at = match on_left != cluster.is_rtl() {
                    true => cluster.bytes.start,
                    false => cluster.bytes.end,
                };
                (at, cluster.bytes.start)
            }
            None => (range.start, range.start),
        };
        // The space a wrapped row broke on is not drawn, so a press past the last letter drawn lands after it, at the row's end.
        let drawn_end = clusters.iter().map(|cluster| cluster.bytes.end).max();
        let at = match drawn_end == Some(at) && self.text[at..range.end].trim().is_empty() {
            true => range.end,
            false => at,
        };
        let affinity = match at == range.end && self.lines.wraps_after(row) {
            true => Affinity::Upstream,
            false => Affinity::Downstream,
        };
        Hit {
            at,
            affinity,
            under,
        }
    }

    /// The stretches of each row `span` covers, as `(row, x0, x1)` in the box, left to right: a row where the span meets text running the other way is covered in several. A row of no characters the span still crosses is given a sliver at the end its paragraph reads to, so a selection reads as continuous through it.
    pub(crate) fn bands(&self, span: Range<usize>, sliver: bool) -> Vec<(usize, f32, f32)> {
        let first = self.row_of(span.start, Affinity::Downstream);
        let last = self.row_of(span.end, Affinity::Upstream).max(first);
        let drawn = self.drawn();
        let mut bands = Vec::new();
        for row in first..=last {
            let range = self.lines.row(row);
            let (from, to) = (span.start.max(range.start), span.end.min(range.end));
            if from > to {
                continue;
            }
            let line = line_of(&drawn, &self.lines, row);
            let mut stretches: Vec<(f32, f32)> = line
                .map_or_else(Vec::new, |line| covered(line, from..to))
                .into_iter()
                .map(|(x0, x1)| (self.held_in(x0), self.held_in(x1)))
                .collect();
            if stretches.is_empty() {
                let x = self.content_x(row, from);
                stretches.push((x, x));
            }
            let runs_on = row < last && !self.lines.wraps_after(row);
            if sliver && runs_on {
                let wide = self.frame.line_h * 0.35;
                match line.is_some_and(|line| line.rtl) {
                    true => {
                        let (x0, x1) = &mut stretches[0];
                        *x0 = x0.min(*x1 - wide);
                    }
                    false => {
                        let (x0, x1) = stretches.last_mut().expect("one stretch at least");
                        *x1 = x1.max(*x0 + wide);
                    }
                }
            }
            let scroll = self.frame.scroll_x;
            bands.extend(
                stretches
                    .into_iter()
                    .map(|(x0, x1)| (row, x0 - scroll, x1 - scroll)),
            );
        }
        bands
    }
}

/// The drawn line `row` of `rows` is: the one that starts where it does.
fn line_of<'l>(drawn: &'l [TextLine], rows: &VisualLines, row: usize) -> Option<&'l TextLine> {
    let start = rows.row(row).start;
    let index = drawn
        .partition_point(|line| line.range.start <= start)
        .checked_sub(1)?;
    drawn.get(index)
}

/// Where a caret at byte `at` stands on `line`: before the cluster that starts there, after the one that ends there at the end of a run, and where the line begins on a line holding nothing.
fn caret_on(line: &TextLine, at: usize) -> f32 {
    if let Some(next) = line.clusters.iter().find(|c| c.bytes.contains(&at)) {
        return next.leading_x();
    }
    if let Some(before) = line.clusters.iter().find(|c| c.bytes.end == at) {
        return before.trailing_x();
    }
    match line.clusters.iter().max_by_key(|c| c.bytes.end) {
        Some(last) if at > line.range.start => last.trailing_x(),
        _ => line.start_x,
    }
}

/// How far `x` is from `cluster`, nothing inside it.
fn distance(cluster: &TextCluster, x: f32) -> f32 {
    (cluster.x - x)
        .max(x - (cluster.x + cluster.width))
        .max(0.0)
}

/// The stretches of `line` its clusters within `span` cover, left to right: one per run of them no cluster outside the span interrupts.
fn covered(line: &TextLine, span: Range<usize>) -> Vec<(f32, f32)> {
    let mut stretches: Vec<(f32, f32)> = Vec::new();
    let mut open = false;
    for cluster in &line.clusters {
        let inside = cluster.bytes.start >= span.start && cluster.bytes.end <= span.end;
        match (inside, open, stretches.last_mut()) {
            (true, true, Some(stretch)) => stretch.1 = cluster.x + cluster.width,
            (true, _, _) => stretches.push((cluster.x, cluster.x + cluster.width)),
            (false, _, _) => {}
        }
        open = inside;
    }
    stretches
}

#[cfg(test)]
#[path = "text_place.test.rs"]
mod tests;
