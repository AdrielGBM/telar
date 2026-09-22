//! A width and a height, with no position: how big a surface is.

/// A 2D extent in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const ZERO: Self = Self::new(0.0, 0.0);

    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    /// The shorter of the two sides.
    pub fn min_side(self) -> f32 {
        self.width.min(self.height)
    }

    /// The longer of the two sides.
    pub fn max_side(self) -> f32 {
        self.width.max(self.height)
    }
}

#[cfg(test)]
#[path = "size_test.rs"]
mod tests;
