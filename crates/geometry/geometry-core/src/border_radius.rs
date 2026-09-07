//! Per-corner corner rounding.

/// The four corner radii of a box, in logical pixels.
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

/// One number is the same radius on every corner, which is what nearly every caller means.
impl From<f32> for BorderRadius {
    fn from(radius: f32) -> Self {
        Self::all(radius)
    }
}

impl Default for BorderRadius {
    fn default() -> Self {
        Self::zero()
    }
}

#[cfg(test)]
#[path = "border_radius_test.rs"]
mod tests;
