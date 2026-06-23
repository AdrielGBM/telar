use geometry_core::{Point, Rect};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_PATH_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone)]
pub enum PathVerb {
    MoveTo(Point),
    LineTo(Point),
    QuadTo {
        ctrl: Point,
        to: Point,
    },
    CubicTo {
        ctrl1: Point,
        ctrl2: Point,
        to: Point,
    },
    Close,
}

#[derive(Debug, Clone)]
pub struct PathData {
    pub id: u64,
    pub(crate) verbs: Vec<PathVerb>,
    // OnceLock is both Send and Sync, required for Arc<PathData>: Send when crossing thread boundaries.
    bounds_cache: std::sync::OnceLock<Option<Rect>>,
}

impl Default for PathData {
    fn default() -> Self {
        Self {
            id: NEXT_PATH_ID.fetch_add(1, Ordering::Relaxed),
            verbs: Vec::new(),
            bounds_cache: std::sync::OnceLock::new(),
        }
    }
}

impl PathData {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn verbs(&self) -> &[PathVerb] {
        &self.verbs
    }

    pub fn move_to(mut self, p: Point) -> Self {
        self.verbs.push(PathVerb::MoveTo(p));
        self.bounds_cache = std::sync::OnceLock::new();
        self
    }

    pub fn line_to(mut self, p: Point) -> Self {
        self.verbs.push(PathVerb::LineTo(p));
        self.bounds_cache = std::sync::OnceLock::new();
        self
    }

    pub fn quad_to(mut self, ctrl: Point, to: Point) -> Self {
        self.verbs.push(PathVerb::QuadTo { ctrl, to });
        self.bounds_cache = std::sync::OnceLock::new();
        self
    }

    pub fn cubic_to(mut self, ctrl1: Point, ctrl2: Point, to: Point) -> Self {
        self.verbs.push(PathVerb::CubicTo { ctrl1, ctrl2, to });
        self.bounds_cache = std::sync::OnceLock::new();
        self
    }

    pub fn close(mut self) -> Self {
        self.verbs.push(PathVerb::Close);
        self.bounds_cache = std::sync::OnceLock::new();
        self
    }

    /// A closed polygon through `points` (first point is the start; the path is closed back to it).
    pub fn polygon(points: &[Point]) -> Self {
        let mut path = Self::new();
        let Some((first, rest)) = points.split_first() else {
            return path;
        };
        path = path.move_to(*first);
        for p in rest {
            path = path.line_to(*p);
        }
        path.close()
    }

    /// A regular `sides`-gon inscribed in a circle of `radius` around `center`. `start_angle_deg` rotates the first vertex (0° points right, increasing clockwise).
    pub fn regular_polygon(center: Point, radius: f32, sides: u32, start_angle_deg: f32) -> Self {
        if sides < 3 {
            return Self::new();
        }
        let start = start_angle_deg.to_radians();
        let step = std::f32::consts::TAU / sides as f32;
        let points: Vec<Point> = (0..sides)
            .map(|i| {
                let a = start + step * i as f32;
                Point::new(center.x + radius * a.cos(), center.y + radius * a.sin())
            })
            .collect();
        Self::polygon(&points)
    }

    /// A closed circle of `radius` around `center`, approximated with four cubic Béziers.
    pub fn circle(center: Point, radius: f32) -> Self {
        // Kappa: control-point offset that makes a cubic Bézier approximate a quarter circle.
        const K: f32 = 0.552_284_8;
        let (cx, cy, r, kr) = (center.x, center.y, radius, radius * K);
        Self::new()
            .move_to(Point::new(cx + r, cy))
            .cubic_to(
                Point::new(cx + r, cy + kr),
                Point::new(cx + kr, cy + r),
                Point::new(cx, cy + r),
            )
            .cubic_to(
                Point::new(cx - kr, cy + r),
                Point::new(cx - r, cy + kr),
                Point::new(cx - r, cy),
            )
            .cubic_to(
                Point::new(cx - r, cy - kr),
                Point::new(cx - kr, cy - r),
                Point::new(cx, cy - r),
            )
            .cubic_to(
                Point::new(cx + kr, cy - r),
                Point::new(cx + r, cy - kr),
                Point::new(cx + r, cy),
            )
            .close()
    }

    pub fn bounds(&self) -> Option<Rect> {
        *self.bounds_cache.get_or_init(|| {
            let mut min_x = f32::INFINITY;
            let mut min_y = f32::INFINITY;
            let mut max_x = f32::NEG_INFINITY;
            let mut max_y = f32::NEG_INFINITY;
            let mut has_geometry = false;

            for verb in &self.verbs {
                match verb {
                    PathVerb::MoveTo(p) | PathVerb::LineTo(p) => {
                        min_x = min_x.min(p.x);
                        min_y = min_y.min(p.y);
                        max_x = max_x.max(p.x);
                        max_y = max_y.max(p.y);
                        has_geometry = true;
                    }
                    PathVerb::QuadTo { ctrl, to } => {
                        // Bézier curves are bounded by their control polygon (convex hull property).
                        for p in &[ctrl, to] {
                            min_x = min_x.min(p.x);
                            min_y = min_y.min(p.y);
                            max_x = max_x.max(p.x);
                            max_y = max_y.max(p.y);
                        }
                        has_geometry = true;
                    }
                    PathVerb::CubicTo { ctrl1, ctrl2, to } => {
                        for p in &[ctrl1, ctrl2, to] {
                            min_x = min_x.min(p.x);
                            min_y = min_y.min(p.y);
                            max_x = max_x.max(p.x);
                            max_y = max_y.max(p.y);
                        }
                        has_geometry = true;
                    }
                    PathVerb::Close => {}
                }
            }

            if has_geometry {
                Some(Rect::new(min_x, min_y, max_x - min_x, max_y - min_y))
            } else {
                None
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_data_new_is_empty() {
        let path = PathData::new();
        assert!(path.verbs().is_empty());
    }

    #[test]
    fn path_data_move_to_adds_verb() {
        let path = PathData::new().move_to(Point::new(0.0, 0.0));
        assert_eq!(path.verbs().len(), 1);
        assert!(matches!(path.verbs()[0], PathVerb::MoveTo(_)));
    }

    #[test]
    fn path_data_line_to_adds_verb() {
        let path = PathData::new()
            .move_to(Point::new(0.0, 0.0))
            .line_to(Point::new(1.0, 1.0));
        assert_eq!(path.verbs().len(), 2);
        assert!(matches!(path.verbs()[1], PathVerb::LineTo(_)));
    }

    #[test]
    fn path_data_close_adds_verb() {
        let path = PathData::new().move_to(Point::new(0.0, 0.0)).close();
        assert!(matches!(path.verbs().last().unwrap(), PathVerb::Close));
    }

    #[test]
    fn path_data_quad_to_adds_verb() {
        let path = PathData::new().quad_to(Point::new(1.0, 0.0), Point::new(2.0, 0.0));
        assert!(matches!(path.verbs()[0], PathVerb::QuadTo { .. }));
    }

    #[test]
    fn path_data_cubic_to_adds_verb() {
        let path = PathData::new().cubic_to(
            Point::new(1.0, 0.0),
            Point::new(2.0, 0.0),
            Point::new(3.0, 0.0),
        );
        assert!(matches!(path.verbs()[0], PathVerb::CubicTo { .. }));
    }

    #[test]
    fn path_data_builder_accumulates_verbs() {
        let path = PathData::new()
            .move_to(Point::new(0.0, 0.0))
            .line_to(Point::new(1.0, 0.0))
            .line_to(Point::new(1.0, 1.0))
            .close();
        assert_eq!(path.verbs().len(), 4);
    }
}
