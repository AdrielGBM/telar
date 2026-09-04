//! [`Lerp`]: what it means to interpolate a value, and the implementations for the geometry types.

use geometry_core::{BorderRadius, Color, Point, Rect, Transform};

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

// Chroma below this (Oklch C) is treated as achromatic; such an endpoint carries the other's hue.
const ACHROMATIC_EPS: f32 = 1e-4;

// Interpolate hue along the shortest arc, carrying the chromatic endpoint's hue when one side is gray.
fn lerp_hue(h1: f32, c1: f32, h2: f32, c2: f32, t: f32) -> f32 {
    let gray1 = c1 < ACHROMATIC_EPS;
    let gray2 = c2 < ACHROMATIC_EPS;
    if gray1 && gray2 {
        return h1;
    }
    if gray1 {
        return h2;
    }
    if gray2 {
        return h1;
    }
    let mut delta = (h2 - h1).rem_euclid(360.0);
    if delta > 180.0 {
        delta -= 360.0;
    }
    (h1 + delta * t).rem_euclid(360.0)
}

impl Lerp for Color {
    fn lerp(&self, other: &Self, t: f32) -> Self {
        // Endpoints return exactly to avoid Oklch round-trip drift at t=0/1.
        if t <= 0.0 {
            return *self;
        }
        if t >= 1.0 {
            return *other;
        }
        let (l1, c1, h1, a1) = self.to_oklcha();
        let (l2, c2, h2, a2) = other.to_oklcha();
        let l = l1.lerp(&l2, t);
        let c = c1.lerp(&c2, t);
        let a = a1.lerp(&a2, t);
        let h = lerp_hue(h1, c1, h2, c2, t);
        Color::from_oklcha(l, c, h, a)
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
mod tests {
    use super::*;

    #[test]
    fn f32_lerp_midpoint() {
        assert!((2.0f32.lerp(&4.0, 0.5) - 3.0).abs() < 1e-6);
    }

    #[test]
    fn point_lerp_is_component_wise() {
        let mid = Point::new(0.0, 10.0).lerp(&Point::new(4.0, 20.0), 0.5);
        assert_eq!(mid, Point::new(2.0, 15.0));
    }

    #[test]
    fn rect_lerp_is_component_wise() {
        let mid = Rect::new(0.0, 0.0, 10.0, 20.0).lerp(&Rect::new(2.0, 4.0, 30.0, 40.0), 0.5);
        assert_eq!(mid, Rect::new(1.0, 2.0, 20.0, 30.0));
    }

    #[test]
    fn border_radius_lerp_is_component_wise() {
        let mid = BorderRadius::all(0.0).lerp(&BorderRadius::all(8.0), 0.25);
        assert_eq!(mid, BorderRadius::all(2.0));
    }

    #[test]
    fn color_lerp_endpoints_are_exact() {
        let red = Color::RED;
        let green = Color::GREEN;
        assert_eq!(red.lerp(&green, 0.0), red);
        assert_eq!(red.lerp(&green, 1.0), green);
    }

    #[test]
    fn color_lerp_gray_to_gray_stays_neutral() {
        let mid = Color::rgb(0.2, 0.2, 0.2).lerp(&Color::rgb(0.8, 0.8, 0.8), 0.5);
        assert!((mid.r - mid.g).abs() < 2e-3, "{mid:?}");
        assert!((mid.g - mid.b).abs() < 2e-3, "{mid:?}");
    }

    #[test]
    fn color_lerp_hue_takes_short_arc() {
        let mid = Color::RED.lerp(&Color::rgb(1.0, 1.0, 0.0), 0.5);
        let (_, chroma, hue, _) = mid.to_oklcha();
        assert!(
            chroma > ACHROMATIC_EPS,
            "midpoint unexpectedly gray: {mid:?}"
        );
        assert!(
            (29.0..=110.0).contains(&hue),
            "hue {hue} left the short arc"
        );
    }

    #[test]
    fn color_lerp_achromatic_endpoint_carries_the_other_hue() {
        let mid = Color::rgb(0.5, 0.5, 0.5).lerp(&Color::RED, 0.5);
        let (_, chroma, hue, _) = mid.to_oklcha();
        let (_, _, red_hue, _) = Color::RED.to_oklcha();
        assert!(chroma > ACHROMATIC_EPS);
        assert!(
            (hue - red_hue).abs() < 1.0,
            "hue {hue} != red hue {red_hue}"
        );
    }
}
