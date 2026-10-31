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
    /// A corner as round as its box allows: [`fitted`](Self::fitted) brings it down to what the box's sides leave room for, so a box with it on every corner is a pill, or a circle when square.
    pub const FULL: f32 = f32::INFINITY;

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

    /// The corners a box of `width` × `height` can draw, as CSS resolves them: where two corners on one side would overlap, every corner shrinks by the same factor until they meet. A negative corner is square.
    pub fn fitted(self, width: f32, height: f32) -> Self {
        // Any corner longer than both sides together overlaps on each side it touches, so FULL stands in as that length and the shared factor stays finite.
        let longest = width.max(0.0) + height.max(0.0);
        let corner = |radius: f32| match radius == Self::FULL {
            true => longest,
            false => radius.max(0.0),
        };
        let fitted = Self {
            top_left: corner(self.top_left),
            top_right: corner(self.top_right),
            bottom_right: corner(self.bottom_right),
            bottom_left: corner(self.bottom_left),
        };
        let room = |side: f32, a: f32, b: f32| match a + b > side {
            true => side / (a + b),
            false => 1.0,
        };
        let factor = room(width, fitted.top_left, fitted.top_right)
            .min(room(width, fitted.bottom_left, fitted.bottom_right))
            .min(room(height, fitted.top_left, fitted.bottom_left))
            .min(room(height, fitted.top_right, fitted.bottom_right));
        match factor < 1.0 {
            true => Self {
                top_left: fitted.top_left * factor,
                top_right: fitted.top_right * factor,
                bottom_right: fitted.bottom_right * factor,
                bottom_left: fitted.bottom_left * factor,
            },
            false => fitted,
        }
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
