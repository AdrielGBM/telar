//! [`TextDecoration`]: a line drawn along the glyphs, placed and sized from the font's own metrics unless the style says otherwise.

use geometry_core::Size;

use super::{Paint, TextLength};

/// Which line a decoration draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DecorationLine {
    #[default]
    None,
    Underline,
}

/// How far below the baseline an underline starts, or how thick it is: the face's own metric, or a length the style names.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum DecorationMetric {
    #[default]
    FromFont,
    Px(f32),
}

impl DecorationMetric {
    /// The metric in pixels, `from_font` standing in for the face's own.
    pub fn or_font(self, from_font: f32) -> f32 {
        match self {
            DecorationMetric::FromFont => from_font,
            DecorationMetric::Px(px) => px,
        }
    }
}

/// A [`DecorationMetric`] as a declaration writes it, with an `em` or a fraction of the surface still to resolve.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum DecorationLength {
    #[default]
    FromFont,
    Length(TextLength),
}

impl DecorationLength {
    pub fn resolve(self, em_base: f32, surface: Size) -> DecorationMetric {
        match self {
            DecorationLength::FromFont => DecorationMetric::FromFont,
            DecorationLength::Length(length) => {
                DecorationMetric::Px(length.resolve(em_base, surface))
            }
        }
    }

    pub fn is_surface_relative(self) -> bool {
        matches!(self, DecorationLength::Length(length) if length.is_surface_relative())
    }

    pub fn on_surface(self, surface: Size) -> Self {
        match self {
            DecorationLength::Length(length) => {
                DecorationLength::Length(length.on_surface(surface))
            }
            from_font => from_font,
        }
    }
}

impl<T: Into<TextLength>> From<T> for DecorationLength {
    fn from(length: T) -> Self {
        DecorationLength::Length(length.into())
    }
}

/// The line drawn along a run of text, as CSS's `text-decoration` longhands describe one: which line, how far below the baseline its top edge sits (`text-underline-offset`), how thick it is and in what paint, the text's own when `color` is `None`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TextDecoration {
    pub line: DecorationLine,
    pub offset: DecorationMetric,
    pub thickness: DecorationMetric,
    pub color: Option<Paint>,
}

impl TextDecoration {
    pub fn underline() -> Self {
        Self {
            line: DecorationLine::Underline,
            ..Self::default()
        }
    }

    pub fn is_drawn(&self) -> bool {
        self.line != DecorationLine::None
    }

    pub fn with_offset(mut self, px: f32) -> Self {
        self.offset = DecorationMetric::Px(px);
        self
    }

    pub fn with_thickness(mut self, px: f32) -> Self {
        self.thickness = DecorationMetric::Px(px);
        self
    }

    pub fn with_color(mut self, color: impl Into<Paint>) -> Self {
        self.color = Some(color.into());
        self
    }
}
