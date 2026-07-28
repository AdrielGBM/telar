mod gradient;
mod paint;
mod scale;
mod shape;

pub use gradient::{Gradient, GradientKind, GradientStop, GradientStops};
pub use paint::{FillRule, LineCap, LineJoin, Paint, Shadow, Stroke};
pub use shape::{PathStyle, RectStyle, ShapeStyle};

/// Horizontal alignment of text within its box. `Start` is the writing-direction start (left in LTR).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    #[default]
    Start,
    Center,
    End,
    Justify,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    pub font_size: f32,
    pub paint: Paint,
    pub shadow: Option<Shadow>,
    /// OpenType weight axis: 400 is normal, 700 is bold. Selects the matching font face.
    pub weight: u16,
    pub italic: bool,
    pub align: TextAlign,
    /// Clamp the text to at most this many lines (`None` = unlimited). Lines beyond it are dropped.
    pub max_lines: Option<u16>,
    /// When clamped by `max_lines`, replace the overflowing tail with an ellipsis (`…`).
    pub ellipsis: bool,
    /// Line height as a multiple of `font_size` (e.g. `1.5`). `None` keeps the shaper's natural line height, so the default renders byte-for-byte as before.
    pub line_height: Option<f32>,
    /// Extra advance in logical pixels added after each glyph. `0.0` uses the font's natural advances.
    pub letter_spacing: f32,
}

impl TextStyle {
    pub fn new(font_size: f32, paint: impl Into<Paint>) -> Self {
        Self {
            font_size,
            paint: paint.into(),
            shadow: None,
            weight: 400,
            italic: false,
            align: TextAlign::Start,
            max_lines: None,
            ellipsis: false,
            line_height: None,
            letter_spacing: 0.0,
        }
    }

    pub fn with_weight(mut self, weight: u16) -> Self {
        self.weight = weight;
        self
    }

    /// Overrides the size a style was built at, so a style carrying theme-resolved weight and slant can be
    /// re-sized without being rebuilt from scratch (and losing them).
    pub fn with_size(mut self, font_size: f32) -> Self {
        self.font_size = font_size;
        self
    }

    /// Drops a shadow behind the glyphs — what keeps text legible over an image the style knows nothing about.
    pub fn with_shadow(mut self, shadow: Shadow) -> Self {
        self.shadow = Some(shadow);
        self
    }

    pub fn with_italic(mut self, italic: bool) -> Self {
        self.italic = italic;
        self
    }

    pub fn with_align(mut self, align: TextAlign) -> Self {
        self.align = align;
        self
    }

    pub fn with_max_lines(mut self, max_lines: u16) -> Self {
        self.max_lines = Some(max_lines);
        self
    }

    pub fn with_ellipsis(mut self, ellipsis: bool) -> Self {
        self.ellipsis = ellipsis;
        self
    }

    pub fn with_line_height(mut self, line_height: f32) -> Self {
        self.line_height = Some(line_height);
        self
    }

    pub fn with_letter_spacing(mut self, letter_spacing: f32) -> Self {
        self.letter_spacing = letter_spacing;
        self
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
    fn text_style_defaults_to_natural_spacing() {
        let style = TextStyle::new(16.0, Color::BLACK);
        assert_eq!(style.line_height, None);
        assert_eq!(style.letter_spacing, 0.0);
    }

    #[test]
    fn text_style_builders_set_spacing() {
        let style = TextStyle::new(16.0, Color::BLACK)
            .with_line_height(1.5)
            .with_letter_spacing(2.0);
        assert_eq!(style.line_height, Some(1.5));
        assert_eq!(style.letter_spacing, 2.0);
    }
}
