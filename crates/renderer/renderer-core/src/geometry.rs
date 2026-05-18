use crate::Color;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
}

impl Default for Rect {
    fn default() -> Self {
        Self::new(0.0, 0.0, 0.0, 0.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BorderRadius {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

impl BorderRadius {
    pub fn all(radius: f32) -> Self {
        Self {
            top_left: radius,
            top_right: radius,
            bottom_right: radius,
            bottom_left: radius,
        }
    }

    pub fn zero() -> Self {
        Self::all(0.0)
    }

    pub fn is_zero(&self) -> bool {
        self.top_left == 0.0
            && self.top_right == 0.0
            && self.bottom_right == 0.0
            && self.bottom_left == 0.0
    }
}

impl Default for BorderRadius {
    fn default() -> Self {
        Self::zero()
    }
}

/// Stroke style for shapes that have corners: paths and rects. Includes `join` to control how corners are rendered. For simple line segments (no corners), use [`LineStyle`] instead.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stroke {
    pub color: Color,
    pub width: f32,
    pub cap: crate::LineCap,
    pub join: crate::LineJoin,
}

impl Stroke {
    pub fn new(color: Color, width: f32) -> Self {
        Self {
            color,
            width,
            cap: crate::LineCap::default(),
            join: crate::LineJoin::default(),
        }
    }

    pub fn with_cap(mut self, cap: crate::LineCap) -> Self {
        self.cap = cap;
        self
    }

    pub fn with_join(mut self, join: crate::LineJoin) -> Self {
        self.join = join;
        self
    }
}

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

#[derive(Debug, Clone, Default)]
pub struct PathData {
    pub(crate) verbs: Vec<PathVerb>,
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
        self
    }

    pub fn line_to(mut self, p: Point) -> Self {
        self.verbs.push(PathVerb::LineTo(p));
        self
    }

    pub fn quad_to(mut self, ctrl: Point, to: Point) -> Self {
        self.verbs.push(PathVerb::QuadTo { ctrl, to });
        self
    }

    pub fn cubic_to(mut self, ctrl1: Point, ctrl2: Point, to: Point) -> Self {
        self.verbs.push(PathVerb::CubicTo { ctrl1, ctrl2, to });
        self
    }

    pub fn close(mut self) -> Self {
        self.verbs.push(PathVerb::Close);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_new_stores_coordinates() {
        let p = Point::new(3.0, 4.0);
        assert_eq!(p.x, 3.0);
        assert_eq!(p.y, 4.0);
    }

    #[test]
    fn rect_new_stores_fields() {
        let r = Rect::new(1.0, 2.0, 10.0, 20.0);
        assert_eq!(r.x, 1.0);
        assert_eq!(r.y, 2.0);
        assert_eq!(r.width, 10.0);
        assert_eq!(r.height, 20.0);
    }

    #[test]
    fn rect_default_is_zero() {
        let r = Rect::default();
        assert_eq!(r.x, 0.0);
        assert_eq!(r.y, 0.0);
        assert_eq!(r.width, 0.0);
        assert_eq!(r.height, 0.0);
    }

    #[test]
    fn border_radius_all_sets_all_corners_equal() {
        let br = BorderRadius::all(8.0);
        assert_eq!(br.top_left, 8.0);
        assert_eq!(br.top_right, 8.0);
        assert_eq!(br.bottom_right, 8.0);
        assert_eq!(br.bottom_left, 8.0);
    }

    #[test]
    fn border_radius_zero_all_corners_are_zero() {
        let br = BorderRadius::zero();
        assert_eq!(br.top_left, 0.0);
        assert_eq!(br.top_right, 0.0);
        assert_eq!(br.bottom_right, 0.0);
        assert_eq!(br.bottom_left, 0.0);
    }

    #[test]
    fn border_radius_zero_is_zero() {
        assert!(BorderRadius::zero().is_zero());
    }

    #[test]
    fn border_radius_non_zero_is_not_zero() {
        assert!(!BorderRadius::all(1.0).is_zero());
    }

    #[test]
    fn border_radius_default_is_zero() {
        assert!(BorderRadius::default().is_zero());
    }

    #[test]
    fn border_radius_partial_non_zero_is_not_zero() {
        let br = BorderRadius {
            top_left: 5.0,
            top_right: 0.0,
            bottom_right: 0.0,
            bottom_left: 0.0,
        };
        assert!(!br.is_zero());
    }

    #[test]
    fn stroke_new_stores_color_and_width() {
        use crate::Color;
        let s = Stroke::new(Color::RED, 3.0);
        assert_eq!(s.color, Color::RED);
        assert_eq!(s.width, 3.0);
    }

    #[test]
    fn stroke_new_defaults_cap_to_butt() {
        use crate::{Color, LineCap};
        let s = Stroke::new(Color::BLACK, 1.0);
        assert_eq!(s.cap, LineCap::Butt);
    }

    #[test]
    fn stroke_new_defaults_join_to_miter() {
        use crate::{Color, LineJoin};
        let s = Stroke::new(Color::BLACK, 1.0);
        assert_eq!(s.join, LineJoin::Miter);
    }

    #[test]
    fn stroke_with_cap_sets_cap() {
        use crate::{Color, LineCap};
        let s = Stroke::new(Color::BLACK, 1.0).with_cap(LineCap::Square);
        assert_eq!(s.cap, LineCap::Square);
    }

    #[test]
    fn stroke_with_join_sets_join() {
        use crate::{Color, LineJoin};
        let s = Stroke::new(Color::BLACK, 1.0).with_join(LineJoin::Bevel);
        assert_eq!(s.join, LineJoin::Bevel);
    }

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
