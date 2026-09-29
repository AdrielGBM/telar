//! [`TextLength`]: a size or a spacing of text written the way a box's length is — in pixels, or relative to something that is only known where the text ends up.

use geometry_core::Size;

/// A length a text style declares: pixels, a multiple of the font size it inherits (`em`), or a fraction of the surface, as `sw`, `sh`, `smin` and `smax` are for a box.
///
/// What a renderer draws is always pixels: the tree resolves a declared length where it knows both the size the text inherits and the surface it is on, and keeps it resolved as either changes (see `Declared::over`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TextLength {
    Px(f32),
    /// A multiple of the font size the text inherits. For a spacing, of the text's own resolved size, as CSS `em` is.
    Em(f32),
    SurfaceWidth(f32),
    SurfaceHeight(f32),
    SurfaceMin(f32),
    SurfaceMax(f32),
}

impl TextLength {
    /// This length in pixels, `em` taken against `em_base` and a fraction of the surface against `surface`.
    pub fn resolve(self, em_base: f32, surface: Size) -> f32 {
        match self {
            TextLength::Px(px) => px,
            TextLength::Em(em) => em * em_base,
            TextLength::SurfaceWidth(f) => f * surface.width,
            TextLength::SurfaceHeight(f) => f * surface.height,
            TextLength::SurfaceMin(f) => f * surface.width.min(surface.height),
            TextLength::SurfaceMax(f) => f * surface.width.max(surface.height),
        }
    }

    /// Whether this length means nothing until the surface's size is known.
    pub fn is_surface_relative(self) -> bool {
        !matches!(self, TextLength::Px(_) | TextLength::Em(_))
    }

    /// The same length with a fraction of the surface turned into the pixels it comes to on `surface`, and pixels and `em` left as they are.
    pub fn on_surface(self, surface: Size) -> Self {
        if self.is_surface_relative() {
            TextLength::Px(self.resolve(0.0, surface))
        } else {
            self
        }
    }

    /// The bits a hash needs: the kind and the number.
    pub fn key(self) -> (u8, u32) {
        let (kind, value) = match self {
            TextLength::Px(v) => (0, v),
            TextLength::Em(v) => (1, v),
            TextLength::SurfaceWidth(v) => (2, v),
            TextLength::SurfaceHeight(v) => (3, v),
            TextLength::SurfaceMin(v) => (4, v),
            TextLength::SurfaceMax(v) => (5, v),
        };
        (kind, value.to_bits())
    }
}

impl From<f32> for TextLength {
    fn from(px: f32) -> Self {
        TextLength::Px(px)
    }
}

#[cfg(test)]
#[path = "text_length_test.rs"]
mod tests;
