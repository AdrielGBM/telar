use crate::{BorderRadius, Color, Point, Stroke};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    pub font_size: f32,
    pub color: Color,
}

impl TextStyle {
    pub fn new(font_size: f32, color: Color) -> Self {
        Self { font_size, color }
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinearGradient {
    pub start: Point,
    pub end: Point,
    pub stops: [GradientStop; 4],
    pub stop_count: u8,
}

impl LinearGradient {
    pub fn new(start: Point, end: Point, stops: &[(f32, Color)]) -> Self {
        let stop_count = stops.len().min(4) as u8;
        let mut arr = [GradientStop::new(0.0, Color::TRANSPARENT); 4];
        for (i, &(position, color)) in stops.iter().take(4).enumerate() {
            arr[i] = GradientStop::new(position, color);
        }
        Self {
            start,
            end,
            stops: arr,
            stop_count,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RadialGradient {
    pub center: Point,
    pub radius: f32,
    pub stops: [GradientStop; 4],
    pub stop_count: u8,
}

impl RadialGradient {
    pub fn new(center: Point, radius: f32, stops: &[(f32, Color)]) -> Self {
        let stop_count = stops.len().min(4) as u8;
        let mut arr = [GradientStop::new(0.0, Color::TRANSPARENT); 4];
        for (i, &(position, color)) in stops.iter().take(4).enumerate() {
            arr[i] = GradientStop::new(position, color);
        }
        Self {
            center,
            radius,
            stops: arr,
            stop_count,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FillStyle {
    Solid(Color),
    LinearGradient(LinearGradient),
    RadialGradient(RadialGradient),
}

impl From<Color> for FillStyle {
    fn from(color: Color) -> Self {
        Self::Solid(color)
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
    pub color: Color,
    pub width: f32,
    pub cap: LineCap,
}

impl LineStyle {
    pub fn new(color: Color, width: f32) -> Self {
        Self {
            color,
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RectStyle {
    pub fill: Option<FillStyle>,
    pub stroke: Option<Stroke>,
    pub radius: BorderRadius,
}

impl Default for RectStyle {
    fn default() -> Self {
        Self {
            fill: None,
            stroke: None,
            radius: BorderRadius::default(),
        }
    }
}

impl RectStyle {
    pub fn with_fill(mut self, fill: impl Into<FillStyle>) -> Self {
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
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathStyle {
    pub fill: Option<FillStyle>,
    pub stroke: Option<Stroke>,
    pub fill_rule: FillRule,
}

impl Default for PathStyle {
    fn default() -> Self {
        Self {
            fill: None,
            stroke: None,
            fill_rule: FillRule::default(),
        }
    }
}

impl PathStyle {
    pub fn with_fill(mut self, fill: impl Into<FillStyle>) -> Self {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_style_from_color_creates_solid() {
        let color = Color::GREEN;
        let fill: FillStyle = color.into();
        assert_eq!(fill, FillStyle::Solid(color));
    }

    #[test]
    fn linear_gradient_new_stores_stops() {
        let p1 = Point::new(0.0, 0.0);
        let p2 = Point::new(1.0, 0.0);
        let g = LinearGradient::new(p1, p2, &[(0.0, Color::BLACK), (1.0, Color::WHITE)]);
        assert_eq!(g.stop_count, 2);
        assert_eq!(g.stops[0].color, Color::BLACK);
        assert_eq!(g.stops[1].color, Color::WHITE);
    }

    #[test]
    fn linear_gradient_new_truncates_to_four() {
        let p = Point::new(0.0, 0.0);
        let stops: Vec<(f32, Color)> = (0..6).map(|i| (i as f32 / 5.0, Color::BLACK)).collect();
        let g = LinearGradient::new(p, p, &stops);
        assert_eq!(g.stop_count, 4);
    }

    #[test]
    fn radial_gradient_new_stores_stops() {
        let c = Point::new(0.5, 0.5);
        let g = RadialGradient::new(c, 1.0, &[(0.0, Color::RED), (1.0, Color::TRANSPARENT)]);
        assert_eq!(g.stop_count, 2);
        assert_eq!(g.stops[0].color, Color::RED);
    }

    #[test]
    fn text_style_new_stores_font_size() {
        let style = TextStyle::new(16.0, Color::BLACK);
        assert_eq!(style.font_size, 16.0);
    }

    #[test]
    fn text_style_new_stores_color() {
        let style = TextStyle::new(12.0, Color::WHITE);
        assert_eq!(style.color, Color::WHITE);
    }

    #[test]
    fn line_style_new_stores_color_and_width() {
        let style = LineStyle::new(Color::RED, 2.0);
        assert_eq!(style.color, Color::RED);
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
}
