use geometry_core::Point;

use crate::Color;

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
pub struct TextStyle {
    pub font_size: f32,
    pub paint: Paint,
    pub shadow: Option<Shadow>,
}

impl TextStyle {
    pub fn new(font_size: f32, paint: impl Into<Paint>) -> Self {
        Self {
            font_size,
            paint: paint.into(),
            shadow: None,
        }
    }

    pub fn with_shadow(mut self, shadow: Shadow) -> Self {
        self.shadow = Some(shadow);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradientStop {
    pub position: f32,
    pub color: Color,
}

impl GradientStop {
    pub fn new(position: f32, color: Color) -> Self {
        Self { position, color }
    }
}

/// Up to 8 gradient color stops. Fixed-size array preserves the `Copy` bound.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradientStops {
    stops: [GradientStop; 8],
    stop_count: u8,
}

impl GradientStops {
    pub fn new(stops: &[(f32, Color)]) -> Self {
        debug_assert!(
            stops.len() <= 8,
            "gradient has {} stops, max is 8",
            stops.len()
        );
        let stop_count = stops.len().min(8) as u8;
        let mut arr = [GradientStop::new(0.0, Color::TRANSPARENT); 8];
        for (i, &(position, color)) in stops.iter().take(8).enumerate() {
            arr[i] = GradientStop::new(position, color);
        }
        Self {
            stops: arr,
            stop_count,
        }
    }

    pub fn active(&self) -> &[GradientStop] {
        &self.stops[..self.stop_count as usize]
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GradientKind {
    Linear { start: Point, end: Point },
    Radial { center: Point, radius: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gradient {
    pub kind: GradientKind,
    pub stops: GradientStops,
}

impl Gradient {
    pub fn linear(start: Point, end: Point, stops: &[(f32, Color)]) -> Self {
        Self {
            kind: GradientKind::Linear { start, end },
            stops: GradientStops::new(stops),
        }
    }

    pub fn radial(center: Point, radius: f32, stops: &[(f32, Color)]) -> Self {
        Self {
            kind: GradientKind::Radial { center, radius },
            stops: GradientStops::new(stops),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Paint {
    Solid(Color),
    Gradient(Gradient),
}

impl From<Color> for Paint {
    fn from(color: Color) -> Self {
        Self::Solid(color)
    }
}

impl Paint {
    pub fn solid_color(&self) -> Color {
        match self {
            Paint::Solid(c) => *c,
            Paint::Gradient(g) => g
                .stops
                .active()
                .first()
                .map_or(Color::TRANSPARENT, |s| s.color),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineCap {
    #[default]
    Butt,
    Round,
    Square,
}

/// Stroke style for `DrawCommand::Line` primitives (point-to-point segments)..Does not include `join` because a single segment has no corners. For paths and rects where corners need styling, use [`Stroke`] instead.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineStyle {
    pub paint: Paint,
    pub width: f32,
    pub cap: LineCap,
}

impl LineStyle {
    pub fn new(paint: impl Into<Paint>, width: f32) -> Self {
        Self {
            paint: paint.into(),
            width,
            cap: LineCap::Butt,
        }
    }

    pub fn with_cap(mut self, cap: LineCap) -> Self {
        self.cap = cap;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FillRule {
    #[default]
    Winding,
    EvenOdd,
}

/// Stroke style for shapes that have corners: paths and rects. Includes `join` to control how corners are rendered. For simple line segments (no corners), use [`LineStyle`] instead.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stroke {
    pub paint: Paint,
    pub width: f32,
    pub cap: LineCap,
    pub join: LineJoin,
}

impl Stroke {
    pub fn new(paint: impl Into<Paint>, width: f32) -> Self {
        Self {
            paint: paint.into(),
            width,
            cap: LineCap::default(),
            join: LineJoin::default(),
        }
    }

    pub fn with_cap(mut self, cap: LineCap) -> Self {
        self.cap = cap;
        self
    }

    pub fn with_join(mut self, join: LineJoin) -> Self {
        self.join = join;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadow {
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur_radius: f32,
    pub spread: f32,
    pub color: Color,
}

impl Shadow {
    pub fn new(offset_x: f32, offset_y: f32, blur_radius: f32, color: Color) -> Self {
        Self {
            offset_x,
            offset_y,
            blur_radius,
            spread: 0.0,
            color,
        }
    }

    pub fn with_spread(mut self, spread: f32) -> Self {
        self.spread = spread;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RectStyle {
    pub fill: Option<Paint>,
    pub stroke: Option<Stroke>,
    pub shadow: Option<Shadow>,
    pub radius: BorderRadius,
}

impl RectStyle {
    pub fn with_fill(mut self, fill: impl Into<Paint>) -> Self {
        self.fill = Some(fill.into());
        self
    }

    pub fn with_stroke(mut self, stroke: Stroke) -> Self {
        self.stroke = Some(stroke);
        self
    }

    pub fn with_radius(mut self, radius: BorderRadius) -> Self {
        self.radius = radius;
        self
    }

    pub fn with_shadow(mut self, shadow: Shadow) -> Self {
        self.shadow = Some(shadow);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PathStyle {
    pub fill: Option<Paint>,
    pub stroke: Option<Stroke>,
    pub shadow: Option<Shadow>,
    pub fill_rule: FillRule,
}

impl PathStyle {
    pub fn with_fill(mut self, fill: impl Into<Paint>) -> Self {
        self.fill = Some(fill.into());
        self
    }

    pub fn with_stroke(mut self, stroke: Stroke) -> Self {
        self.stroke = Some(stroke);
        self
    }

    pub fn with_fill_rule(mut self, rule: FillRule) -> Self {
        self.fill_rule = rule;
        self
    }

    pub fn with_shadow(mut self, shadow: Shadow) -> Self {
        self.shadow = Some(shadow);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paint_from_color_creates_solid() {
        let color = Color::GREEN;
        let fill: Paint = color.into();
        assert_eq!(fill, Paint::Solid(color));
    }

    #[test]
    fn gradient_stops_new_stores_stops() {
        let stops = GradientStops::new(&[(0.0, Color::BLACK), (1.0, Color::WHITE)]);
        assert_eq!(stops.active().len(), 2);
        assert_eq!(stops.active()[0].color, Color::BLACK);
        assert_eq!(stops.active()[1].color, Color::WHITE);
    }

    #[test]
    fn gradient_stops_new_truncates_to_eight() {
        let raw: Vec<(f32, Color)> = (0..6).map(|i| (i as f32 / 5.0, Color::BLACK)).collect();
        let stops = GradientStops::new(&raw);
        assert_eq!(stops.active().len(), 6);
    }

    #[test]
    fn gradient_linear_stores_stops() {
        let p1 = Point::new(0.0, 0.0);
        let p2 = Point::new(1.0, 0.0);
        let g = Gradient::linear(p1, p2, &[(0.0, Color::BLACK), (1.0, Color::WHITE)]);
        assert_eq!(g.stops.active().len(), 2);
        assert_eq!(g.stops.active()[0].color, Color::BLACK);
    }

    #[test]
    fn gradient_radial_stores_stops() {
        let c = Point::new(0.5, 0.5);
        let g = Gradient::radial(c, 1.0, &[(0.0, Color::RED), (1.0, Color::TRANSPARENT)]);
        assert_eq!(g.stops.active().len(), 2);
        assert_eq!(g.stops.active()[0].color, Color::RED);
    }

    #[test]
    fn text_style_new_stores_font_size() {
        let style = TextStyle::new(16.0, Color::BLACK);
        assert_eq!(style.font_size, 16.0);
    }

    #[test]
    fn text_style_new_stores_color() {
        let style = TextStyle::new(12.0, Color::WHITE);
        assert_eq!(style.paint, Paint::Solid(Color::WHITE));
    }

    #[test]
    fn line_style_new_stores_color_and_width() {
        let style = LineStyle::new(Color::RED, 2.0);
        assert_eq!(style.paint, Paint::Solid(Color::RED));
        assert_eq!(style.width, 2.0);
    }

    #[test]
    fn line_style_new_defaults_cap_to_butt() {
        let style = LineStyle::new(Color::BLACK, 1.0);
        assert_eq!(style.cap, LineCap::Butt);
    }

    #[test]
    fn line_style_with_cap_sets_cap() {
        let style = LineStyle::new(Color::BLACK, 1.0).with_cap(LineCap::Round);
        assert_eq!(style.cap, LineCap::Round);
    }

    #[test]
    fn line_cap_default_is_butt() {
        assert_eq!(LineCap::default(), LineCap::Butt);
    }

    #[test]
    fn line_join_default_is_miter() {
        assert_eq!(LineJoin::default(), LineJoin::Miter);
    }

    #[test]
    fn fill_rule_default_is_winding() {
        assert_eq!(FillRule::default(), FillRule::Winding);
    }

    #[test]
    fn shadow_new_stores_fields_and_zero_spread() {
        let shadow = Shadow::new(2.0, 4.0, 8.0, Color::BLACK);
        assert_eq!(shadow.offset_x, 2.0);
        assert_eq!(shadow.offset_y, 4.0);
        assert_eq!(shadow.blur_radius, 8.0);
        assert_eq!(shadow.spread, 0.0);
        assert_eq!(shadow.color, Color::BLACK);
    }

    #[test]
    fn shadow_with_spread_sets_spread() {
        let shadow = Shadow::new(0.0, 0.0, 4.0, Color::RED).with_spread(6.0);
        assert_eq!(shadow.spread, 6.0);
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
        let s = Stroke::new(Color::RED, 3.0);
        assert_eq!(s.paint, Paint::Solid(Color::RED));
        assert_eq!(s.width, 3.0);
    }

    #[test]
    fn stroke_new_defaults_cap_to_butt() {
        let s = Stroke::new(Color::BLACK, 1.0);
        assert_eq!(s.cap, LineCap::Butt);
    }

    #[test]
    fn stroke_new_defaults_join_to_miter() {
        let s = Stroke::new(Color::BLACK, 1.0);
        assert_eq!(s.join, LineJoin::Miter);
    }

    #[test]
    fn stroke_with_cap_sets_cap() {
        let s = Stroke::new(Color::BLACK, 1.0).with_cap(LineCap::Square);
        assert_eq!(s.cap, LineCap::Square);
    }

    #[test]
    fn stroke_with_join_sets_join() {
        let s = Stroke::new(Color::BLACK, 1.0).with_join(LineJoin::Bevel);
        assert_eq!(s.join, LineJoin::Bevel);
    }
}
