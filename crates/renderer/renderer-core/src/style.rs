use crate::{BorderRadius, Color, Stroke};

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
pub enum FillStyle {
    Solid(Color),
}

impl FillStyle {
    pub fn color(&self) -> Color {
        match self {
            Self::Solid(c) => *c,
        }
    }
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
    fn fill_style_solid_color_returns_color() {
        let color = Color::RED;
        let fill = FillStyle::Solid(color);
        assert_eq!(fill.color(), color);
    }

    #[test]
    fn fill_style_from_color_creates_solid() {
        let color = Color::GREEN;
        let fill: FillStyle = color.into();
        assert_eq!(fill, FillStyle::Solid(color));
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
