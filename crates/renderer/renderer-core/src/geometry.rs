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
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
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
    pub verbs: Vec<PathVerb>,
}

impl PathData {
    pub fn new() -> Self {
        Self::default()
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
