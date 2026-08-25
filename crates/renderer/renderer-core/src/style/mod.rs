use std::num::NonZeroU16;
use std::sync::Arc;

mod declared;
mod gradient;
mod paint;
mod scale;
mod shape;

pub use declared::{Declared, Span};
pub use gradient::{Gradient, GradientKind, GradientStop, GradientStops};
pub use paint::{FillRule, LineCap, LineJoin, Paint, Shadow, Stroke};
pub use shape::{BorderWidths, PathStyle, RectStyle, ShapeStyle, border_inner_shape};

/// Horizontal alignment of text within its box. `Start` is the writing-direction start (left in LTR).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    #[default]
    Start,
    Center,
    End,
    Justify,
}

/// Which grid the glyphs are rasterized onto.
///
/// An axis of the style, like weight or slant — not a mode the whole renderer enters. Shaping,
/// wrapping, bidi and the font stack are the same either way; only where a glyph lands and how its
/// coverage is resolved change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GlyphRaster {
    /// Subpixel origins and blended coverage: the sharpest text a screen can show at UI sizes.
    #[default]
    Smooth,
    /// Whole-pixel origins and coverage resolved to on or off.
    ///
    /// What a font drawn on a pixel grid needs: a glyph shared between two columns, or an edge left
    /// half-lit, is the grid the artist drew being taken apart. With a face designed at the size it is
    /// used, this reproduces a bitmap font's output without Telar growing a second font format —
    /// cosmic-text still shapes, wraps and falls back exactly as before.
    Pixel,
}

/// How text ends when it does not fit.
///
/// One decision, so one type. Split across a `max_lines: Option<u16>` and an `ellipsis: bool` it had two
/// states that meant nothing and were accepted anyway: an ellipsis with no line limit, which the clamp
/// returns before ever consulting, and a limit of zero lines, which three separate call sites each defended
/// against with the same `.filter(|&n| n > 0)`. Neither is representable now, and the three filters are gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Clamp {
    #[default]
    None,
    Lines {
        max: NonZeroU16,
        /// Mark the cut with `…` rather than letting the text stop mid-word.
        ellipsis: bool,
    },
}

impl Clamp {
    /// The line limit, for the shaper's "did this overflow" question.
    pub fn max_lines(self) -> Option<usize> {
        match self {
            Clamp::None => None,
            Clamp::Lines { max, .. } => Some(max.get() as usize),
        }
    }

    pub fn ellipsis(self) -> bool {
        matches!(self, Clamp::Lines { ellipsis: true, .. })
    }

    /// A clamp to `max` lines, or [`None`](Clamp::None) when `max` is zero — the state that used to be
    /// representable and meaningless.
    pub fn lines(max: u16, ellipsis: bool) -> Self {
        match NonZeroU16::new(max) {
            Some(max) => Clamp::Lines { max, ellipsis },
            None => Clamp::None,
        }
    }
}

/// Which face a run of text shapes in.
///
/// [`SansSerif`](Self::SansSerif) is whatever the surface's font configuration resolves it to — the
/// application's own family, an OEM stack, or the platform's default — and it is a *value* rather than a
/// `None` on purpose: a property that flows down a tree has to be able to tell "nobody named a family"
/// from "somebody named the default one", and an `Option` collapses those into the same thing.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum FontFamily {
    #[default]
    SansSerif,
    Named(Arc<str>),
}

impl<T: AsRef<str>> From<T> for FontFamily {
    fn from(name: T) -> Self {
        FontFamily::Named(name.as_ref().into())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextStyle {
    pub font_size: f32,
    pub paint: Paint,
    pub shadow: Option<Shadow>,
    /// The face to shape in. See [`FontFamily`].
    pub font_family: FontFamily,
    /// OpenType weight axis: 400 is normal, 700 is bold. Selects the matching font face.
    pub weight: u16,
    pub italic: bool,
    pub align: TextAlign,
    /// How the text ends when it does not fit. See [`Clamp`].
    pub clamp: Clamp,
    /// Line height as a multiple of `font_size` (e.g. `1.5`). `None` keeps the shaper's natural line height, so the default renders byte-for-byte as before.
    pub line_height: Option<f32>,
    /// Extra advance in logical pixels added after each glyph. `0.0` uses the font's natural advances.
    pub letter_spacing: f32,
    /// Which grid the glyphs land on. See [`GlyphRaster`].
    pub raster: GlyphRaster,
    /// Keep the text on one line whatever width it is given, instead of wrapping into the box.
    ///
    /// What a label in a toolbar, a status bar or a table cell wants: it is a *name*, and a name broken
    /// across two lines to fit a column reads as two things. Wrapping is the right default for prose and the
    /// wrong one for everything that is really a token, so it is a property of the style rather than of the
    /// box — the same text is a label in one place and a paragraph in another.
    pub no_wrap: bool,
}

impl TextStyle {
    pub fn new(font_size: f32, paint: impl Into<Paint>) -> Self {
        Self {
            font_size,
            paint: paint.into(),
            shadow: None,
            font_family: FontFamily::SansSerif,
            weight: 400,
            italic: false,
            align: TextAlign::Start,
            clamp: Clamp::None,
            line_height: None,
            letter_spacing: 0.0,
            raster: GlyphRaster::Smooth,
            no_wrap: false,
        }
    }

    /// Keeps the text on one line; see [`no_wrap`](Self::no_wrap).
    pub fn with_no_wrap(mut self, no_wrap: bool) -> Self {
        self.no_wrap = no_wrap;
        self
    }

    pub fn with_font_size(mut self, font_size: f32) -> Self {
        self.font_size = font_size;
        self
    }

    pub fn with_paint(mut self, paint: impl Into<Paint>) -> Self {
        self.paint = paint.into();
        self
    }

    pub fn with_weight(mut self, weight: u16) -> Self {
        self.weight = weight;
        self
    }

    /// Shapes this text in `family` instead of the configured sans-serif face.
    pub fn with_font_family(mut self, family: impl Into<FontFamily>) -> Self {
        self.font_family = family.into();
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

    /// Clamps the text to `max` lines, marking the cut with `…` when `ellipsis`. A `max` of zero is no clamp.
    pub fn with_clamp(mut self, max: u16, ellipsis: bool) -> Self {
        self.clamp = Clamp::lines(max, ellipsis);
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

    /// Puts the glyphs on whole pixels with coverage resolved to on or off. See [`GlyphRaster`].
    pub fn with_raster(mut self, raster: GlyphRaster) -> Self {
        self.raster = raster;
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
