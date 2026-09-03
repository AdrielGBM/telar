//! Boxes, borders, lines and paths, in the characters a terminal has for them.

use geometry_core::{Point, Rect};
use renderer_core::{Color, FillRule, PathData, PathStyle, PathVerb, RectStyle, Stroke};

use super::geom::{cell_center, mapped, sample};
use super::{CellRect, Painter};
use crate::cell::{Attrs, Grapheme};

/// Which way a border line leaves a cell. A cell's set of directions is what picks its character, so
/// corners, edges, junctions and a one-cell-wide box all fall out of the same table.
mod side {
    pub const UP: u8 = 1 << 0;
    pub const DOWN: u8 = 1 << 1;
    pub const LEFT: u8 = 1 << 2;
    pub const RIGHT: u8 = 1 << 3;
}

/// The box-drawing characters for one weight of line. Unicode has no rounded heavy corners, so the heavy
/// set uses square ones and a rounded heavy border simply reads as square.
struct BoxChars {
    horizontal: char,
    vertical: char,
    top_left: char,
    top_right: char,
    bottom_left: char,
    bottom_right: char,
    tee_down: char,
    tee_up: char,
    tee_right: char,
    tee_left: char,
    cross: char,
}

const LIGHT: BoxChars = BoxChars {
    horizontal: '─',
    vertical: '│',
    top_left: '┌',
    top_right: '┐',
    bottom_left: '└',
    bottom_right: '┘',
    tee_down: '┬',
    tee_up: '┴',
    tee_right: '├',
    tee_left: '┤',
    cross: '┼',
};

const ROUND: BoxChars = BoxChars {
    horizontal: '─',
    vertical: '│',
    top_left: '╭',
    top_right: '╮',
    bottom_left: '╰',
    bottom_right: '╯',
    tee_down: '┬',
    tee_up: '┴',
    tee_right: '├',
    tee_left: '┤',
    cross: '┼',
};

const HEAVY: BoxChars = BoxChars {
    horizontal: '━',
    vertical: '┃',
    top_left: '┏',
    top_right: '┓',
    bottom_left: '┗',
    bottom_right: '┛',
    tee_down: '┳',
    tee_up: '┻',
    tee_right: '┣',
    tee_left: '┫',
    cross: '╋',
};

impl BoxChars {
    fn of(self_mask: u8, chars: &BoxChars) -> Option<char> {
        use side::*;
        Some(match self_mask {
            m if m == RIGHT | DOWN => chars.top_left,
            m if m == LEFT | DOWN => chars.top_right,
            m if m == RIGHT | UP => chars.bottom_left,
            m if m == LEFT | UP => chars.bottom_right,
            m if m == LEFT | RIGHT | DOWN => chars.tee_down,
            m if m == LEFT | RIGHT | UP => chars.tee_up,
            m if m == UP | DOWN | RIGHT => chars.tee_right,
            m if m == UP | DOWN | LEFT => chars.tee_left,
            m if m == UP | DOWN | LEFT | RIGHT => chars.cross,
            m if m & (LEFT | RIGHT) != 0 && m & (UP | DOWN) == 0 => chars.horizontal,
            m if m & (UP | DOWN) != 0 && m & (LEFT | RIGHT) == 0 => chars.vertical,
            _ => return None,
        })
    }
}

impl Painter<'_> {
    pub(crate) fn rect(&mut self, rect: Rect, style: &RectStyle) {
        let cells = self.cells_of(rect);
        if cells.is_empty() {
            self.hairline(rect, style);
            return;
        }
        // The shadow is dropped rather than approximated. A terminal has no sub-cell falloff, so the only
        // thing to draw is a hard band of colour offset from the box — which reads as a second box.
        if let Some(fill) = &style.fill {
            let paint = mapped(fill, self.matrix(), self.scale());
            for row in cells.row0..cells.row1 {
                for col in cells.col0..cells.col1 {
                    let p = cell_center(col, row, self.cell);
                    self.blend_bg(col, row, sample(&paint, p.x, p.y));
                }
            }
        }
        if let Some((paint, widths)) = style.painted_border() {
            let paint = mapped(&paint, self.matrix(), self.scale());
            self.border(cells, widths, &paint, style.radius);
        }
    }

    fn border(
        &mut self,
        cells: CellRect,
        widths: [f32; 4],
        paint: &renderer_core::Paint,
        radius: renderer_core::BorderRadius,
    ) {
        // A border's weight is categorical here: a hairline and a two-pixel rule are both one cell of line,
        // so the only distinction a terminal can carry is light versus heavy.
        let thickest = widths.iter().copied().fold(0.0f32, f32::max) * self.scale();
        let chars = if thickest >= self.cell.width * 0.5 {
            &HEAVY
        } else if radius.is_zero() {
            &LIGHT
        } else {
            &ROUND
        };

        let mut mask = vec![0u8; cells.cols() as usize * cells.rows() as usize];
        let idx = |col: i32, row: i32| {
            (row - cells.row0) as usize * cells.cols() as usize + (col - cells.col0) as usize
        };
        let last_col = cells.col1 - 1;
        let last_row = cells.row1 - 1;

        let horizontal = |mask: &mut Vec<u8>, row: i32| {
            for col in cells.col0..cells.col1 {
                let mut m = 0;
                if col > cells.col0 || cells.col0 == last_col {
                    m |= side::LEFT;
                }
                if col < last_col || cells.col0 == last_col {
                    m |= side::RIGHT;
                }
                mask[idx(col, row)] |= m;
            }
        };
        if widths[0] > 0.0 {
            horizontal(&mut mask, cells.row0);
        }
        if widths[2] > 0.0 && last_row != cells.row0 {
            horizontal(&mut mask, last_row);
        } else if widths[2] > 0.0 {
            horizontal(&mut mask, cells.row0);
        }

        let vertical = |mask: &mut Vec<u8>, col: i32| {
            for row in cells.row0..cells.row1 {
                let mut m = 0;
                if row > cells.row0 || cells.row0 == last_row {
                    m |= side::UP;
                }
                if row < last_row || cells.row0 == last_row {
                    m |= side::DOWN;
                }
                mask[idx(col, row)] |= m;
            }
        };
        if widths[3] > 0.0 {
            vertical(&mut mask, cells.col0);
        }
        if widths[1] > 0.0 && last_col != cells.col0 {
            vertical(&mut mask, last_col);
        } else if widths[1] > 0.0 {
            vertical(&mut mask, cells.col0);
        }

        for row in cells.row0..cells.row1 {
            for col in cells.col0..cells.col1 {
                let Some(ch) = BoxChars::of(mask[idx(col, row)], chars) else {
                    continue;
                };
                let p = cell_center(col, row, self.cell);
                let color = sample(paint, p.x, p.y);
                self.put_glyph(col, row, Grapheme::from(ch), color, Attrs::NONE);
            }
        }
    }

    pub(crate) fn line(&mut self, p1: Point, p2: Point, style: &Stroke) {
        let paint = mapped(&style.paint, self.matrix(), self.scale());
        let t = geometry_core::Transform::from_array(self.matrix());
        let a = t.apply(p1);
        let b = t.apply(p2);
        let color = sample(&paint, (a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
        self.stroke_segment(a, b, color);
    }

    /// One straight run, walked in cells. The character says which way the run is heading, which is as much
    /// direction as a cell can carry.
    fn stroke_segment(&mut self, a: Point, b: Point, color: Color) {
        let from = (self.cell.col_at(a.x), self.cell.row_at(a.y));
        let to = (self.cell.col_at(b.x), self.cell.row_at(b.y));
        let (dc, dr) = (to.0 - from.0, to.1 - from.1);
        let ch = run_char(dc, dr);
        let steps = dc.abs().max(dr.abs()).max(1);
        for i in 0..=steps {
            let k = i as f32 / steps as f32;
            let col = from.0 + (dc as f32 * k).round() as i32;
            let row = from.1 + (dr as f32 * k).round() as i32;
            if !self.clipped_in(col, row) {
                continue;
            }
            self.put_glyph(col, row, Grapheme::from(ch), color, Attrs::NONE);
        }
    }

    pub(crate) fn path(&mut self, data: &PathData, style: &PathStyle) {
        let t = geometry_core::Transform::from_array(self.matrix());
        let mut polygons: Vec<Vec<Point>> = Vec::new();
        flatten(data, &t, self.scale(), &mut polygons);
        if polygons.is_empty() {
            return;
        }
        if let Some(fill) = &style.fill {
            let paint = mapped(fill, self.matrix(), self.scale());
            self.fill_polygons(&polygons, &paint, style.fill_rule);
        }
        if let Some(stroke) = &style.stroke {
            let paint = mapped(&stroke.paint, self.matrix(), self.scale());
            for polygon in &polygons {
                for pair in polygon.windows(2) {
                    let mid =
                        Point::new((pair[0].x + pair[1].x) * 0.5, (pair[0].y + pair[1].y) * 0.5);
                    let color = sample(&paint, mid.x, mid.y);
                    self.stroke_segment(pair[0], pair[1], color);
                }
            }
        }
    }

    /// Scanline fill at cell resolution: a cell belongs to the shape when the shape covers its centre.
    fn fill_polygons(
        &mut self,
        polygons: &[Vec<Point>],
        paint: &renderer_core::Paint,
        rule: FillRule,
    ) {
        let mut min = Point::new(f32::INFINITY, f32::INFINITY);
        let mut max = Point::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
        for polygon in polygons {
            for p in polygon {
                min = Point::new(min.x.min(p.x), min.y.min(p.y));
                max = Point::new(max.x.max(p.x), max.y.max(p.y));
            }
        }
        if !min.x.is_finite() || !max.x.is_finite() {
            return;
        }
        let bounds = self.cells_of_window(Rect::new(
            min.x,
            min.y,
            (max.x - min.x).max(0.0),
            (max.y - min.y).max(0.0),
        ));
        let mut crossings: Vec<(f32, i32)> = Vec::new();
        for row in bounds.row0..bounds.row1 {
            let y = cell_center(0, row, self.cell).y;
            crossings.clear();
            for polygon in polygons {
                for pair in polygon.windows(2) {
                    let (a, b) = (pair[0], pair[1]);
                    if (a.y <= y) == (b.y <= y) {
                        continue;
                    }
                    let k = (y - a.y) / (b.y - a.y);
                    crossings.push((a.x + (b.x - a.x) * k, if b.y > a.y { 1 } else { -1 }));
                }
            }
            if crossings.is_empty() {
                continue;
            }
            crossings.sort_by(|a, b| a.0.total_cmp(&b.0));
            for col in bounds.col0..bounds.col1 {
                let x = cell_center(col, 0, self.cell).x;
                let inside = match rule {
                    FillRule::EvenOdd => {
                        crossings.iter().filter(|(cx, _)| *cx <= x).count() % 2 == 1
                    }
                    FillRule::Winding => {
                        crossings
                            .iter()
                            .filter(|(cx, _)| *cx <= x)
                            .map(|(_, dir)| *dir)
                            .sum::<i32>()
                            != 0
                    }
                };
                if !inside || !self.clipped_in(col, row) {
                    continue;
                }
                let p = cell_center(col, row, self.cell);
                let color = sample(paint, p.x, p.y);
                self.blend_bg(col, row, color);
            }
        }
    }

    /// Cells for a rect already in window space — the path pipeline maps its own points, so it must not be
    /// mapped a second time.
    fn cells_of_window(&self, rect: Rect) -> CellRect {
        CellRect::of(rect, self.cell).intersect(CellRect {
            col0: 0,
            row0: 0,
            col1: self.buf.cols() as i32,
            row1: self.buf.rows() as i32,
        })
    }
}

/// Flattens a path's curves into polylines in window space. The tolerance is a fraction of a cell, since a
/// curve smoother than one cell is indistinguishable from a straight line here.
fn flatten(
    data: &PathData,
    transform: &geometry_core::Transform,
    scale: f32,
    out: &mut Vec<Vec<Point>>,
) {
    let steps_for = |len: f32| ((len * scale / 4.0).ceil() as usize).clamp(1, 24);
    let mut current: Vec<Point> = Vec::new();
    let mut start = Point::new(0.0, 0.0);
    let mut at = start;
    for verb in data.verbs() {
        match verb {
            PathVerb::MoveTo(p) => {
                if current.len() > 1 {
                    out.push(std::mem::take(&mut current));
                } else {
                    current.clear();
                }
                start = *p;
                at = *p;
                current.push(transform.apply(*p));
            }
            PathVerb::LineTo(p) => {
                at = *p;
                current.push(transform.apply(*p));
            }
            PathVerb::QuadTo { ctrl, to } => {
                let n = steps_for(
                    (ctrl.x - at.x).hypot(ctrl.y - at.y) + (to.x - ctrl.x).hypot(to.y - ctrl.y),
                );
                for i in 1..=n {
                    let t = i as f32 / n as f32;
                    current.push(transform.apply(quad(at, *ctrl, *to, t)));
                }
                at = *to;
            }
            PathVerb::CubicTo { ctrl1, ctrl2, to } => {
                let n = steps_for(
                    (ctrl1.x - at.x).hypot(ctrl1.y - at.y)
                        + (ctrl2.x - ctrl1.x).hypot(ctrl2.y - ctrl1.y)
                        + (to.x - ctrl2.x).hypot(to.y - ctrl2.y),
                );
                for i in 1..=n {
                    let t = i as f32 / n as f32;
                    current.push(transform.apply(cubic(at, *ctrl1, *ctrl2, *to, t)));
                }
                at = *to;
            }
            PathVerb::Close => {
                current.push(transform.apply(start));
                at = start;
                if current.len() > 1 {
                    out.push(std::mem::take(&mut current));
                } else {
                    current.clear();
                }
                current.push(transform.apply(start));
            }
        }
    }
    if current.len() > 1 {
        out.push(current);
    }
}

fn quad(a: Point, c: Point, b: Point, t: f32) -> Point {
    let u = 1.0 - t;
    Point::new(
        u * u * a.x + 2.0 * u * t * c.x + t * t * b.x,
        u * u * a.y + 2.0 * u * t * c.y + t * t * b.y,
    )
}

fn cubic(a: Point, c1: Point, c2: Point, b: Point, t: f32) -> Point {
    let u = 1.0 - t;
    let (u2, t2) = (u * u, t * t);
    Point::new(
        u2 * u * a.x + 3.0 * u2 * t * c1.x + 3.0 * u * t2 * c2.x + t2 * t * b.x,
        u2 * u * a.y + 3.0 * u2 * t * c1.y + 3.0 * u * t2 * c2.y + t2 * t * b.y,
    )
}

/// The character for a run heading `(dc, dr)` cells. A run within 30° of an axis reads as that axis; the
/// rest are the two diagonals.
fn run_char(dc: i32, dr: i32) -> char {
    match (dc.abs(), dr.abs()) {
        (0, 0) => '·',
        (c, r) if r * 2 <= c => '─',
        (c, r) if c * 2 <= r => '│',
        _ if (dc > 0) == (dr > 0) => '╲',
        _ => '╱',
    }
}

impl Painter<'_> {
    /// A rect thinner than a cell, drawn as the line it is.
    ///
    /// Rounding both edges to the same column is the right answer for a box — it is what makes neighbours
    /// tile — but it erases everything a UI draws at hairline width: a text caret, a one-pixel divider, a
    /// separator between rows. Those are not boxes that happen to be small; they are lines, and a terminal
    /// has characters for lines.
    fn hairline(&mut self, rect: Rect, style: &RectStyle) {
        let Some(fill) = &style.fill else {
            return;
        };
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        let mapped_rect = renderer_core::transform_clip_rect(self.matrix(), rect);
        let full = CellRect::of(mapped_rect, self.cell);
        // One cell in whichever axis collapsed, keeping the other as it laid out.
        let cells = CellRect {
            col0: full.col0,
            row0: full.row0,
            col1: full.col1.max(full.col0 + 1),
            row1: full.row1.max(full.row0 + 1),
        };
        let glyph = match (full.cols() == 0, full.rows() == 0) {
            (true, true) => '·',
            (true, false) => '│',
            (false, true) => '─',
            (false, false) => return,
        };
        let paint = mapped(fill, self.matrix(), self.scale());
        for row in cells.row0..cells.row1 {
            for col in cells.col0..cells.col1 {
                if !self.clipped_in(col, row) {
                    continue;
                }
                let p = cell_center(col, row, self.cell);
                let color = sample(&paint, p.x, p.y);
                self.put_glyph(col, row, Grapheme::from(glyph), color, Attrs::NONE);
            }
        }
    }
}
