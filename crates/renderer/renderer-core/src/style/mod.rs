//! The style types: what a box, a path and a run of text are painted with.

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
pub use shape::{Border, PathStyle, RectStyle, ShapeStyle, border_inner_shape};

/// Horizontal alignment of text within its box. `Start` is the writing-direction start (left in LTR).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    #[default]
    Start,
    Center,
    End,
    Justify,
}

/// How samples meet the pixel grid — for glyphs and for pictures alike.
///
/// One property, because it is one question. It used to be two enums with no connection and disagreeing defaults: `Raster::{Smooth, Pixel}` for text and `Raster::{Nearest, Linear}` for images, so an application wanting a crisp pixel grid had to know both names and set both, everywhere. A pixel-art application says `raster:pixel` once and every glyph and every picture beneath it lands on whole pixels.
///
/// An axis of the style, like weight or slant — not a mode the whole renderer enters. Shaping, wrapping, bidi and the font stack are the same either way; only where a sample lands and how its coverage is resolved change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Raster {
    /// Subpixel origins and blended coverage: the sharpest text a screen can show at UI sizes, and a bilinear filter for a picture drawn at anything but its own size.
    #[default]
    Smooth,
    /// Whole-pixel origins and coverage resolved to on or off; nearest-neighbour for a picture.
    ///
    /// What artwork drawn on a pixel grid needs: a glyph shared between two columns, or an edge left half-lit, is the grid the artist drew being taken apart. With a face designed at the size it is used, this reproduces a bitmap font's output without Telar growing a second font format — cosmic-text still shapes, wraps and falls back exactly as before.
    Pixel,
}

/// The slant of a face.
///
/// Three-valued rather than an `italic: bool`, because the shaper already models all three and only two were ever reachable: cosmic-text has `Style::Oblique`, and nothing could ask for it. A boolean named after a three-valued property would have been a worse name than the one it replaced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
    /// A slanted upright face, where italic is a differently drawn one.
    Oblique,
}

/// Whether text wraps into its box or keeps one line whatever width it is given.
///
/// [`NoWrap`](Self::NoWrap) is what a label in a toolbar, a status bar or a table cell wants: it is a *name*, and a name broken across two lines to fit a column reads as two things. Wrapping is right for prose and wrong for everything that is really a token, so it belongs to the style rather than the box — the same text is a label in one place and a paragraph in another.
///
/// Positive rather than a `no_wrap: bool`, and named apart from the container's `wrap:` (which is flex-wrap) that its old spelling was one character away from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TextWrap {
    #[default]
    Wrap,
    NoWrap,
}

/// The height of a line, as a multiple of the font size.
///
/// A value rather than an `Option<f32>`, because a property that flows down a tree has to be able to tell "nobody said" from "somebody said: the font's own" — and an `Option` inside a partial style would nest two different meanings of nothing. CSS spells the same distinction `line-height: normal`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum LineHeight {
    /// Whatever the shaper's natural line height is for the face and size.
    #[default]
    Natural,
    Times(f32),
}

impl LineHeight {
    /// The multiple to apply, or `None` for the shaper's own.
    pub fn factor(self) -> Option<f32> {
        match self {
            LineHeight::Natural => None,
            LineHeight::Times(n) => Some(n),
        }
    }
}

/// A shadow cast behind the glyphs, or none.
///
/// Carries its own absence for the same reason [`LineHeight`] does. Named apart from `RectStyle`'s shadow, which is the identical type on a field of the identical name — but one inherits and the other must never, so under a cascade a bare `shadow:` could not be resolved.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TextShadow {
    #[default]
    None,
    Cast(Shadow),
}

impl TextShadow {
    pub fn cast(self) -> Option<Shadow> {
        match self {
            TextShadow::None => None,
            TextShadow::Cast(shadow) => Some(shadow),
        }
    }
}

/// How text ends when it does not fit.
///
/// One decision, so one type. Split across a `max_lines: Option<u16>` and an `ellipsis: bool` it had two states that meant nothing and were accepted anyway: an ellipsis with no line limit, which the clamp returns before ever consulting, and a limit of zero lines, which three separate call sites each defended against with the same `.filter(|&n| n > 0)`. Neither is representable now, and the three filters are gone.
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

    /// A clamp to `max` lines, or [`None`](Clamp::None) when `max` is zero — the state that used to be representable and meaningless.
    pub fn lines(max: u16, ellipsis: bool) -> Self {
        match NonZeroU16::new(max) {
            Some(max) => Clamp::Lines { max, ellipsis },
            None => Clamp::None,
        }
    }
}

/// Which face a run of text shapes in.
///
/// [`SansSerif`](Self::SansSerif) is whatever the surface's font configuration resolves it to — the application's own family, an OEM stack, or the platform's default — and it is a *value* rather than a `None` on purpose: a property that flows down a tree has to be able to tell "nobody named a family" from "somebody named the default one", and an `Option` collapses those into the same thing.
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
/// The complete style a run of text is drawn with: face, size, weight, colour, alignment and wrapping.
pub struct TextStyle {
    pub font_size: f32,
    /// The ink the glyphs are painted with. A `Paint` rather than a `Color`, the way modern CSS `color:` also accepts more than a plain colour.
    pub color: Paint,
    /// A shadow behind the glyphs. See [`TextShadow`].
    pub text_shadow: TextShadow,
    /// The face to shape in. See [`FontFamily`].
    pub font_family: FontFamily,
    /// OpenType weight axis: 400 is normal, 700 is bold. Selects the matching font face.
    pub font_weight: u16,
    /// The slant of the face. See [`FontStyle`].
    pub font_style: FontStyle,
    pub text_align: TextAlign,
    /// How the text ends when it does not fit. See [`Clamp`].
    pub clamp: Clamp,
    /// The height of a line, as a multiple of `font_size`. See [`LineHeight`].
    pub line_height: LineHeight,
    /// Extra advance in logical pixels added after each glyph. `0.0` uses the font's natural advances.
    pub letter_spacing: f32,
    /// Which grid the glyphs land on. See [`Raster`].
    pub raster: Raster,
    /// Whether the text wraps into its box. See [`TextWrap`].
    pub text_wrap: TextWrap,
}

impl TextStyle {
    pub fn new(font_size: f32, color: impl Into<Paint>) -> Self {
        Self {
            font_size,
            color: color.into(),
            text_shadow: TextShadow::None,
            font_family: FontFamily::SansSerif,
            font_weight: 400,
            font_style: FontStyle::Normal,
            text_align: TextAlign::Start,
            clamp: Clamp::None,
            line_height: LineHeight::Natural,
            letter_spacing: 0.0,
            raster: Raster::Smooth,
            text_wrap: TextWrap::Wrap,
        }
    }

    /// Keeps the text on one line; see [`TextWrap`].
    pub fn with_text_wrap(mut self, text_wrap: TextWrap) -> Self {
        self.text_wrap = text_wrap;
        self
    }

    pub fn with_font_size(mut self, font_size: f32) -> Self {
        self.font_size = font_size;
        self
    }

    pub fn with_color(mut self, color: impl Into<Paint>) -> Self {
        self.color = color.into();
        self
    }

    pub fn with_font_weight(mut self, font_weight: u16) -> Self {
        self.font_weight = font_weight;
        self
    }

    /// Shapes this text in `family` instead of the configured sans-serif face.
    pub fn with_font_family(mut self, family: impl Into<FontFamily>) -> Self {
        self.font_family = family.into();
        self
    }

    /// Drops a shadow behind the glyphs — what keeps text legible over an image the style knows nothing about.
    pub fn with_text_shadow(mut self, shadow: Shadow) -> Self {
        self.text_shadow = TextShadow::Cast(shadow);
        self
    }

    pub fn with_font_style(mut self, font_style: FontStyle) -> Self {
        self.font_style = font_style;
        self
    }

    pub fn with_text_align(mut self, text_align: TextAlign) -> Self {
        self.text_align = text_align;
        self
    }

    /// Clamps the text to `max` lines, marking the cut with `…` when `ellipsis`. A `max` of zero is no clamp.
    pub fn with_clamp(mut self, max: u16, ellipsis: bool) -> Self {
        self.clamp = Clamp::lines(max, ellipsis);
        self
    }

    pub fn with_line_height(mut self, times: f32) -> Self {
        self.line_height = LineHeight::Times(times);
        self
    }

    pub fn with_letter_spacing(mut self, letter_spacing: f32) -> Self {
        self.letter_spacing = letter_spacing;
        self
    }

    /// Puts the glyphs on whole pixels with coverage resolved to on or off. See [`Raster`].
    pub fn with_raster(mut self, raster: Raster) -> Self {
        self.raster = raster;
        self
    }
}

/// Turning a logical-pixel value into physical pixels, for backends that do not fold the scale into a shader.
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
        assert_eq!(style.color, Paint::Solid(Color::WHITE));
    }

    #[test]
    fn text_style_defaults_to_natural_spacing() {
        let style = TextStyle::new(16.0, Color::BLACK);
        assert_eq!(style.line_height, LineHeight::Natural);
        assert_eq!(style.letter_spacing, 0.0);
    }

    #[test]
    fn text_style_builders_set_spacing() {
        let style = TextStyle::new(16.0, Color::BLACK)
            .with_line_height(1.5)
            .with_letter_spacing(2.0);
        assert_eq!(style.line_height, LineHeight::Times(1.5));
        assert_eq!(style.letter_spacing, 2.0);
    }
}
