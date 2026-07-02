mod gradient;
mod paint;
mod scale;
mod shape;

pub use gradient::{Gradient, GradientKind, GradientStop, GradientStops};
pub use paint::{FillRule, LineCap, LineJoin, Paint, Shadow, Stroke};
pub use shape::{PathStyle, RectStyle, ShapeStyle};

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
}

pub trait Scale: Sized {
    fn scale(self, sf: f32) -> Self;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Color;

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
}
