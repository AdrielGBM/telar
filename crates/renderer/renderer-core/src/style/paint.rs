//! [`Paint`]: a solid colour or a gradient, and what every fill and stroke resolves to.

use crate::Color;

use super::gradient::Gradient;

#[derive(Debug, Clone, Copy, PartialEq)]
/// What a fill or stroke is drawn with: a solid colour or a gradient.
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

    /// The same paint at a fraction of the opacity it already had.
    ///
    /// Scales the alpha rather than setting one, so quieting something already quiet makes it quieter rather than louder — which is what a caller means when the paint is one it was handed rather than one it chose.
    pub fn faded(self, factor: f32) -> Self {
        match self {
            Paint::Solid(c) => Paint::Solid(c.with_alpha(c.a * factor)),
            Paint::Gradient(g) => Paint::Gradient(Gradient {
                stops: g.stops.faded(factor),
                ..g
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
/// How a stroke ends.
pub enum LineCap {
    #[default]
    Butt,
    Round,
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
/// How two stroke segments meet.
pub enum LineJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
/// Which side of a self-intersecting path counts as inside.
pub enum FillRule {
    #[default]
    Winding,
    EvenOdd,
}

/// Stroke style for drawing primitives. Includes `join` to control how corners are rendered in paths and rects; for line segments `join` is unused and defaults to `Miter`.
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
/// A drop shadow: its colour, offset, blur and spread.
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

#[cfg(test)]
#[path = "paint_test.rs"]
mod tests;
