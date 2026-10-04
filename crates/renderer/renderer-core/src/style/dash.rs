//! [`Dash`]: the drawn-and-skipped pattern a stroke follows, and the split into dashes for a stroker that only draws solid lines.

use std::hash::{Hash, Hasher};

use geometry_core::Point;

use crate::{PathData, PathVerb};

/// How far a flattened curve may stray from the curve when the caller names no usable tolerance.
const DEFAULT_TOLERANCE: f32 = 0.1;

/// The most straight pieces one curve is flattened into, however long it is.
const MAX_CURVE_PIECES: u32 = 1024;

/// A dash pattern: lengths that alternate drawn and skipped along a stroke, starting with a drawn one, and how far into the pattern the stroke begins.
///
/// Normalised when built, so every backend reads the same pattern: an odd list is repeated into an even one, as SVG and canvas do, and the offset is wrapped into a single period. Fixed-size, like [`GradientStops`](crate::GradientStops), so a [`Stroke`](crate::Stroke) stays `Copy`.
#[derive(Debug, Clone, Copy)]
pub struct Dash {
    lengths: [f32; Dash::CAPACITY],
    count: u8,
    offset: f32,
}

impl Dash {
    /// The most lengths a pattern holds once an odd list has been repeated.
    pub const CAPACITY: usize = 16;

    /// A pattern from `pattern` and `offset`, or `None` when it would draw no dashes: an empty list, a length that is negative or not finite, lengths that sum to zero, an offset that is not finite, or more than [`CAPACITY`](Self::CAPACITY) lengths. A stroke without a pattern is solid, which is what SVG and canvas draw for such a list too.
    ///
    /// `offset` is how far into the pattern the stroke starts, so a positive one shifts the dashes back along the stroke; a negative one is allowed.
    pub fn new(pattern: &[f32], offset: f32) -> Option<Self> {
        let count = if pattern.len() % 2 == 1 {
            pattern.len() * 2
        } else {
            pattern.len()
        };
        if count == 0
            || count > Self::CAPACITY
            || !offset.is_finite()
            || pattern
                .iter()
                .any(|length| !length.is_finite() || *length < 0.0)
        {
            return None;
        }
        let mut lengths = [0.0; Self::CAPACITY];
        for (slot, length) in lengths.iter_mut().zip(pattern.iter().cycle()).take(count) {
            // Adding zero turns a negative zero positive, so equal patterns are equal bit for bit.
            *slot = *length + 0.0;
        }
        let period: f32 = lengths[..count].iter().sum();
        if !(period > 0.0 && period.is_finite()) {
            return None;
        }
        let wrapped = offset.rem_euclid(period) + 0.0;
        Some(Self {
            lengths,
            count: count as u8,
            offset: if wrapped < period { wrapped } else { 0.0 },
        })
    }

    /// The drawn and skipped lengths, drawn first, always an even number of them.
    pub fn lengths(&self) -> &[f32] {
        &self.lengths[..self.count as usize]
    }

    /// How far into the pattern the stroke starts, within `0.0..period()`.
    pub fn offset(&self) -> f32 {
        self.offset
    }

    /// The length of one repetition of the pattern.
    pub fn period(&self) -> f32 {
        self.lengths().iter().sum()
    }

    /// The pattern with every length and the offset multiplied by `factor`, or `None` when that leaves no pattern, as a factor of zero does.
    pub fn scaled(&self, factor: f32) -> Option<Self> {
        let mut lengths = [0.0; Self::CAPACITY];
        for (slot, length) in lengths.iter_mut().zip(self.lengths()) {
            *slot = length * factor;
        }
        Self::new(&lengths[..self.count as usize], self.offset * factor)
    }

    /// `path` cut into its dashes, for a stroker that only draws solid lines. Curves are flattened first, to within `tolerance` of the curve, so the result holds straight segments only.
    ///
    /// Follows SVG: the pattern restarts at every subpath, and on a closed subpath the dash that runs through its start is one dash joined there rather than two capped ones; a closed subpath the pattern never leaves stays closed. A drawn length of zero is a dash with no length, which a round or square cap still shows as a dot.
    pub fn split(&self, path: &PathData, tolerance: f32) -> PathData {
        let mut verbs = Vec::new();
        for contour in contours(path, tolerance) {
            self.walk(&contour.points, contour.closed, |dash, closed| {
                verbs.push(PathVerb::MoveTo(dash[0]));
                verbs.extend(dash[1..].iter().map(|point| PathVerb::LineTo(*point)));
                if closed {
                    verbs.push(PathVerb::Close);
                }
            });
        }
        PathData::from_verbs(verbs)
    }

    /// The dashes along the straight segment from `from` to `to`, as the two ends of each.
    pub fn split_segment(&self, from: Point, to: Point) -> Vec<(Point, Point)> {
        let mut dashes = Vec::new();
        self.walk(&[from, to], false, |dash, _| {
            dashes.push((dash[0], dash[dash.len() - 1]));
        });
        dashes
    }

    /// Where the pattern stands at the start of a subpath: the length it is in, and how much of that length is left.
    fn start(&self) -> (usize, f32) {
        let lengths = self.lengths();
        let mut phase = self.offset;
        let mut index = 0;
        // `phase > 0` keeps a drawn zero at the very start, which is a dot, rather than skipping it.
        while phase > 0.0 && phase >= lengths[index] {
            phase -= lengths[index];
            index = (index + 1) % lengths.len();
        }
        (index, lengths[index] - phase)
    }

    /// Walks the polyline through `points`, back to its first point when `closed`, and hands each dash to `emit` with whether it is a whole closed loop.
    fn walk(&self, points: &[Point], closed: bool, mut emit: impl FnMut(&[Point], bool)) {
        let Some(&first_point) = points.first() else {
            return;
        };
        let lengths = self.lengths();
        let (mut index, mut remaining) = self.start();
        let mut dash: Vec<Point> = Vec::new();
        if index % 2 == 0 {
            dash.push(first_point);
        }
        // On a closed subpath that starts inside a dash, that dash is held back to be joined to the one the subpath ends in.
        let mut holding_first = closed && index % 2 == 0;
        let mut first: Option<Vec<Point>> = None;
        let mut interrupted = false;

        let closing = closed.then(|| (points[points.len() - 1], first_point));
        let segments = points.windows(2).map(|w| (w[0], w[1])).chain(closing);
        for (a, b) in segments {
            let length = distance(a, b);
            let mut travelled = 0.0;
            while remaining <= length - travelled {
                travelled += remaining;
                let at = lerp(a, b, travelled, length);
                if index % 2 == 0 {
                    push_distinct(&mut dash, at);
                    if dash.len() == 1 {
                        dash.push(at);
                    }
                    let finished = std::mem::take(&mut dash);
                    if holding_first {
                        holding_first = false;
                        first = Some(finished);
                    } else {
                        emit(&finished, false);
                    }
                }
                interrupted = true;
                index = (index + 1) % lengths.len();
                remaining = lengths[index];
                if index % 2 == 0 {
                    dash.push(at);
                }
            }
            remaining -= length - travelled;
            if index % 2 == 0 {
                push_distinct(&mut dash, b);
            }
        }

        let ends_drawn = index % 2 == 0 && dash.len() >= 2;
        match (ends_drawn, first) {
            (true, _) if closed && !interrupted => {
                if dash.len() > 2 && dash.last() == dash.first() {
                    dash.pop();
                }
                emit(&dash, true);
            }
            (true, Some(first)) => {
                dash.extend(first.into_iter().skip(1));
                emit(&dash, false);
            }
            (true, None) => emit(&dash, false),
            (false, Some(first)) => emit(&first, false),
            (false, None) => {}
        }
    }
}

impl PartialEq for Dash {
    fn eq(&self, other: &Self) -> bool {
        self.offset.to_bits() == other.offset.to_bits()
            && self.lengths().len() == other.lengths().len()
            && self
                .lengths()
                .iter()
                .zip(other.lengths())
                .all(|(a, b)| a.to_bits() == b.to_bits())
    }
}

// Bitwise equality is float equality here: construction rejects NaN and turns every negative zero positive.
impl Eq for Dash {}

impl Hash for Dash {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_usize(self.lengths().len());
        for length in self.lengths() {
            state.write_u32(length.to_bits());
        }
        state.write_u32(self.offset.to_bits());
    }
}

/// One subpath, flattened.
struct Contour {
    points: Vec<Point>,
    closed: bool,
}

fn contours(path: &PathData, tolerance: f32) -> Vec<Contour> {
    let tolerance = if tolerance > 0.0 && tolerance.is_finite() {
        tolerance
    } else {
        DEFAULT_TOLERANCE
    };
    let mut contours = Vec::new();
    let mut points: Vec<Point> = Vec::new();
    let mut start = Point::new(0.0, 0.0);
    let finish = |contours: &mut Vec<Contour>, points: &mut Vec<Point>, closed: bool| {
        let points = std::mem::take(points);
        if points.len() >= 2 {
            contours.push(Contour { points, closed });
        }
    };
    for verb in path.verbs() {
        // A segment after a close, with no move between, starts where the closed subpath did.
        if !matches!(verb, PathVerb::MoveTo(_) | PathVerb::Close) && points.is_empty() {
            points.push(start);
        }
        let from = points.last().copied().unwrap_or(start);
        match *verb {
            PathVerb::MoveTo(p) => {
                finish(&mut contours, &mut points, false);
                start = p;
                points.push(p);
            }
            PathVerb::LineTo(p) => points.push(p),
            PathVerb::QuadTo { ctrl, to } => {
                let bend = deviation(from, ctrl, to);
                let pieces = pieces((bend / (4.0 * tolerance)).sqrt());
                for i in 1..=pieces {
                    let t = i as f32 / pieces as f32;
                    let u = 1.0 - t;
                    points.push(Point::new(
                        u * u * from.x + 2.0 * u * t * ctrl.x + t * t * to.x,
                        u * u * from.y + 2.0 * u * t * ctrl.y + t * t * to.y,
                    ));
                }
            }
            PathVerb::CubicTo { ctrl1, ctrl2, to } => {
                let bend = deviation(from, ctrl1, ctrl2).max(deviation(ctrl1, ctrl2, to));
                let pieces = pieces((3.0 * bend / (4.0 * tolerance)).sqrt());
                for i in 1..=pieces {
                    let t = i as f32 / pieces as f32;
                    let u = 1.0 - t;
                    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                    points.push(Point::new(
                        a * from.x + b * ctrl1.x + c * ctrl2.x + d * to.x,
                        a * from.y + b * ctrl1.y + c * ctrl2.y + d * to.y,
                    ));
                }
            }
            PathVerb::Close => finish(&mut contours, &mut points, true),
        }
    }
    finish(&mut contours, &mut points, false);
    contours
}

/// How far the middle of three control points bends away from the straight line through the other two: the second difference that bounds how far a flattened curve strays.
fn deviation(a: Point, b: Point, c: Point) -> f32 {
    let x = a.x - 2.0 * b.x + c.x;
    let y = a.y - 2.0 * b.y + c.y;
    (x * x + y * y).sqrt()
}

fn pieces(estimate: f32) -> u32 {
    if estimate.is_finite() {
        (estimate.ceil() as u32).clamp(1, MAX_CURVE_PIECES)
    } else {
        1
    }
}

fn distance(a: Point, b: Point) -> f32 {
    (b.x - a.x).hypot(b.y - a.y)
}

fn lerp(a: Point, b: Point, travelled: f32, length: f32) -> Point {
    if length <= 0.0 {
        return a;
    }
    let t = (travelled / length).clamp(0.0, 1.0);
    Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}

fn push_distinct(points: &mut Vec<Point>, point: Point) {
    if points.last() != Some(&point) {
        points.push(point);
    }
}

#[cfg(test)]
#[path = "dash_test.rs"]
mod tests;
