//! [`FontFit`]: a text size asked for by the length its line should come to, rather than by a size.

use geometry_core::Size;

use super::TextLength;

/// How wide a fitted line is asked to be.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FitWidth {
    /// A fraction of the content width of the box holding the text, as `%` is for a box: `0.6` is `fit(60%)`.
    Containing(f32),
    /// A length, `em` taken against the size the text inherits.
    Length(TextLength),
}

impl FitWidth {
    /// The width in pixels, or `None` for a fraction of a box whose width is not known.
    pub fn resolve(
        self,
        em_base: f32,
        surface: Size,
        containing_width: Option<f32>,
    ) -> Option<f32> {
        match self {
            FitWidth::Containing(fraction) => containing_width.map(|width| fraction * width),
            FitWidth::Length(length) => Some(length.resolve(em_base, surface)),
        }
    }
}

impl From<TextLength> for FitWidth {
    fn from(length: TextLength) -> Self {
        FitWidth::Length(length)
    }
}

impl From<f32> for FitWidth {
    fn from(px: f32) -> Self {
        FitWidth::Length(TextLength::Px(px))
    }
}

/// A size that sets a text's single line to a width, and no taller than an optional height: `font_size:fit(60%, max_height:56sh)`.
///
/// Layout resolves it where the width is known, through the installed [`TextMetrics`](crate::TextMetrics), and the line is scaled whole: what the text declares in `em` (its tracking) scales with it, and it never wraps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontFit {
    pub width: FitWidth,
    pub max_height: Option<TextLength>,
}

impl FontFit {
    pub fn new(width: impl Into<FitWidth>) -> Self {
        Self {
            width: width.into(),
            max_height: None,
        }
    }

    /// `fit(60%)`: a fraction of the content width of the box holding the text.
    pub fn containing(fraction: f32) -> Self {
        Self::new(FitWidth::Containing(fraction))
    }

    pub fn with_max_height(mut self, max_height: impl Into<TextLength>) -> Self {
        self.max_height = Some(max_height.into());
        self
    }

    /// Whether resolving this reads the surface's size.
    pub fn uses_surface(&self) -> bool {
        let width = match self.width {
            FitWidth::Containing(_) => false,
            FitWidth::Length(length) => length.is_surface_relative(),
        };
        width || self.max_height.is_some_and(TextLength::is_surface_relative)
    }

    /// The `(width, max_height)` in pixels this asks for, or `None` while the width means nothing yet.
    pub fn target(
        &self,
        em_base: f32,
        surface: Size,
        containing_width: Option<f32>,
    ) -> Option<(f32, Option<f32>)> {
        let width = self.width.resolve(em_base, surface, containing_width)?;
        let max_height = self
            .max_height
            .map(|height| height.resolve(em_base, surface));
        Some((width, max_height))
    }
}

#[cfg(test)]
#[path = "font_fit_test.rs"]
mod tests;
