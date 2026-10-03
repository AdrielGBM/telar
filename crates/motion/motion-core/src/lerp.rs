//! [`Lerp`]: what it means to interpolate a value, and the implementations for the geometry types.

use geometry_core::{BorderRadius, Color, Point, Rect, Transform};
use renderer_core::FontVariations;

/// Interpolation plus the minimal vector-space operations the engine needs.
///
/// `lerp` powers tweens; `add`/`sub`/`scale`/`zero`/`magnitude_sq` let the spring integrate in value space (component-wise). Keeping both on one trait means `Animated<T>` needs only `T: Lerp` for tweens and springs alike. Spring velocity is stored as a `T` in value space, so f32 springs are exact physical springs and vector types integrate per component; `Color` springs therefore run in sRGB-component space while `Color` tweens use the perceptual Oklch path in `lerp`.
pub trait Lerp: Clone {
    /// Interpolate between `self` (t=0) and `other` (t=1).
    fn lerp(&self, other: &Self, t: f32) -> Self;

    /// Component-wise sum; the spring accumulates displacement and velocity here.
    fn add(&self, other: &Self) -> Self;

    /// Component-wise difference (displacement from `other` toward `self`).
    fn sub(&self, other: &Self) -> Self;

    /// Component-wise scalar multiply.
    fn scale(&self, factor: f32) -> Self;

    /// Additive identity, used as the initial spring velocity.
    fn zero() -> Self;

    /// Squared Euclidean magnitude, used for spring settle and change thresholds.
    fn magnitude_sq(&self) -> f32;
}

impl Lerp for f32 {
    fn lerp(&self, other: &Self, t: f32) -> Self {
        self + (other - self) * t
    }
    fn add(&self, other: &Self) -> Self {
        self + other
    }
    fn sub(&self, other: &Self) -> Self {
        self - other
    }
    fn scale(&self, factor: f32) -> Self {
        self * factor
    }
    fn zero() -> Self {
        0.0
    }
    fn magnitude_sq(&self) -> f32 {
        self * self
    }
}

impl Lerp for Point {
    fn lerp(&self, other: &Self, t: f32) -> Self {
        Point::new(self.x.lerp(&other.x, t), self.y.lerp(&other.y, t))
    }
    fn add(&self, other: &Self) -> Self {
        Point::new(self.x + other.x, self.y + other.y)
    }
    fn sub(&self, other: &Self) -> Self {
        Point::new(self.x - other.x, self.y - other.y)
    }
    fn scale(&self, factor: f32) -> Self {
        Point::new(self.x * factor, self.y * factor)
    }
    fn zero() -> Self {
        Point::new(0.0, 0.0)
    }
    fn magnitude_sq(&self) -> f32 {
        self.x * self.x + self.y * self.y
    }
}

impl Lerp for Rect {
    fn lerp(&self, other: &Self, t: f32) -> Self {
        Rect::new(
            self.x.lerp(&other.x, t),
            self.y.lerp(&other.y, t),
            self.width.lerp(&other.width, t),
            self.height.lerp(&other.height, t),
        )
    }
    fn add(&self, other: &Self) -> Self {
        Rect::new(
            self.x + other.x,
            self.y + other.y,
            self.width + other.width,
            self.height + other.height,
        )
    }
    fn sub(&self, other: &Self) -> Self {
        Rect::new(
            self.x - other.x,
            self.y - other.y,
            self.width - other.width,
            self.height - other.height,
        )
    }
    fn scale(&self, factor: f32) -> Self {
        Rect::new(
            self.x * factor,
            self.y * factor,
            self.width * factor,
            self.height * factor,
        )
    }
    fn zero() -> Self {
        Rect::new(0.0, 0.0, 0.0, 0.0)
    }
    fn magnitude_sq(&self) -> f32 {
        self.x * self.x + self.y * self.y + self.width * self.width + self.height * self.height
    }
}

impl Lerp for Transform {
    fn lerp(&self, other: &Self, t: f32) -> Self {
        Transform {
            a: self.a.lerp(&other.a, t),
            b: self.b.lerp(&other.b, t),
            c: self.c.lerp(&other.c, t),
            d: self.d.lerp(&other.d, t),
            e: self.e.lerp(&other.e, t),
            f: self.f.lerp(&other.f, t),
        }
    }
    fn add(&self, other: &Self) -> Self {
        Transform {
            a: self.a + other.a,
            b: self.b + other.b,
            c: self.c + other.c,
            d: self.d + other.d,
            e: self.e + other.e,
            f: self.f + other.f,
        }
    }
    fn sub(&self, other: &Self) -> Self {
        Transform {
            a: self.a - other.a,
            b: self.b - other.b,
            c: self.c - other.c,
            d: self.d - other.d,
            e: self.e - other.e,
            f: self.f - other.f,
        }
    }
    fn scale(&self, factor: f32) -> Self {
        Transform {
            a: self.a * factor,
            b: self.b * factor,
            c: self.c * factor,
            d: self.d * factor,
            e: self.e * factor,
            f: self.f * factor,
        }
    }
    fn zero() -> Self {
        Transform {
            a: 0.0,
            b: 0.0,
            c: 0.0,
            d: 0.0,
            e: 0.0,
            f: 0.0,
        }
    }
    fn magnitude_sq(&self) -> f32 {
        self.a * self.a
            + self.b * self.b
            + self.c * self.c
            + self.d * self.d
            + self.e * self.e
            + self.f * self.f
    }
}

impl Lerp for BorderRadius {
    fn lerp(&self, other: &Self, t: f32) -> Self {
        BorderRadius {
            top_left: self.top_left.lerp(&other.top_left, t),
            top_right: self.top_right.lerp(&other.top_right, t),
            bottom_right: self.bottom_right.lerp(&other.bottom_right, t),
            bottom_left: self.bottom_left.lerp(&other.bottom_left, t),
        }
    }
    fn add(&self, other: &Self) -> Self {
        BorderRadius {
            top_left: self.top_left + other.top_left,
            top_right: self.top_right + other.top_right,
            bottom_right: self.bottom_right + other.bottom_right,
            bottom_left: self.bottom_left + other.bottom_left,
        }
    }
    fn sub(&self, other: &Self) -> Self {
        BorderRadius {
            top_left: self.top_left - other.top_left,
            top_right: self.top_right - other.top_right,
            bottom_right: self.bottom_right - other.bottom_right,
            bottom_left: self.bottom_left - other.bottom_left,
        }
    }
    fn scale(&self, factor: f32) -> Self {
        BorderRadius {
            top_left: self.top_left * factor,
            top_right: self.top_right * factor,
            bottom_right: self.bottom_right * factor,
            bottom_left: self.bottom_left * factor,
        }
    }
    fn zero() -> Self {
        BorderRadius::all(0.0)
    }
    fn magnitude_sq(&self) -> f32 {
        self.top_left * self.top_left
            + self.top_right * self.top_right
            + self.bottom_right * self.bottom_right
            + self.bottom_left * self.bottom_left
    }
}

impl Lerp for Color {
    fn lerp(&self, other: &Self, t: f32) -> Self {
        self.mix(*other, t)
    }
    // Spring integration for Color runs in sRGB-component space (see trait docs).
    fn add(&self, other: &Self) -> Self {
        Color::rgba(
            self.r + other.r,
            self.g + other.g,
            self.b + other.b,
            self.a + other.a,
        )
    }
    fn sub(&self, other: &Self) -> Self {
        Color::rgba(
            self.r - other.r,
            self.g - other.g,
            self.b - other.b,
            self.a - other.a,
        )
    }
    fn scale(&self, factor: f32) -> Self {
        Color::rgba(
            self.r * factor,
            self.g * factor,
            self.b * factor,
            self.a * factor,
        )
    }
    fn zero() -> Self {
        Color::rgba(0.0, 0.0, 0.0, 0.0)
    }
    fn magnitude_sq(&self) -> f32 {
        self.r * self.r + self.g * self.g + self.b * self.b + self.a * self.a
    }
}

#[cfg(test)]
#[path = "lerp_test.rs"]
mod tests;

/// Axis by axis: an axis only one side names is taken as `0.0` on the other for the arithmetic a spring runs, and holds its value through a tween (see [`FontVariations::lerp`]).
impl Lerp for FontVariations {
    fn lerp(&self, other: &Self, t: f32) -> Self {
        FontVariations::lerp(self, other, t)
    }
    fn add(&self, other: &Self) -> Self {
        combine(self, other, |a, b| a + b)
    }
    fn sub(&self, other: &Self) -> Self {
        combine(self, other, |a, b| a - b)
    }
    fn scale(&self, factor: f32) -> Self {
        FontVariations::with_values_of(self, self.values().map(|value| value * factor))
    }
    fn zero() -> Self {
        FontVariations::new()
    }
    fn magnitude_sq(&self) -> f32 {
        self.values().map(|value| value * value).sum()
    }
}

fn combine(a: &FontVariations, b: &FontVariations, op: impl Fn(f32, f32) -> f32) -> FontVariations {
    let mut out = FontVariations::new();
    for (tag, value) in a.iter() {
        out = out.with(tag, op(value, b.get(tag).unwrap_or(0.0)));
    }
    for (tag, value) in b.iter() {
        if a.get(tag).is_none() {
            out = out.with(tag, op(0.0, value));
        }
    }
    out
}
