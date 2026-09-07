//! 2D affine transforms, and composing them without multiplying matrices by hand.

use crate::point::Point;

/// A 2D affine transform stored as a 2×3 matrix `[a, b, c, d, e, f]`, mapping a point `(x, y)` to `(a*x + c*y + e, b*x + d*y + f)`. This is the same `[f32; 6]` layout consumed by `RenderNode::transform_with`, so `to_array()` plugs in directly. Compose with [`Transform::then`] instead of multiplying matrices by hand.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    pub const IDENTITY: Transform = Transform {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    /// Translate by `(tx, ty)`.
    pub fn translate(tx: f32, ty: f32) -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: tx,
            f: ty,
        }
    }

    /// Scale by `(sx, sy)` keeping the point `(cx, cy)` fixed.
    pub fn scale_around(sx: f32, sy: f32, cx: f32, cy: f32) -> Self {
        Self {
            a: sx,
            b: 0.0,
            c: 0.0,
            d: sy,
            e: cx - sx * cx,
            f: cy - sy * cy,
        }
    }

    /// Rotate by `angle_deg` degrees keeping the point `(cx, cy)` fixed.
    pub fn rotate_around(angle_deg: f32, cx: f32, cy: f32) -> Self {
        let a = angle_deg.to_radians();
        let cos = a.cos();
        let sin = a.sin();
        Self {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            e: cx - cx * cos + cy * sin,
            f: cy - cx * sin - cy * cos,
        }
    }

    /// Returns the transform that applies `self` first and then `next` (`next ∘ self`), so `a.then(b).apply(p) == b.apply(a.apply(p))`.
    pub fn then(self, next: Transform) -> Transform {
        Transform {
            a: next.a * self.a + next.c * self.b,
            b: next.b * self.a + next.d * self.b,
            c: next.a * self.c + next.c * self.d,
            d: next.b * self.c + next.d * self.d,
            e: next.a * self.e + next.c * self.f + next.e,
            f: next.b * self.e + next.d * self.f + next.f,
        }
    }

    pub fn apply(&self, p: Point) -> Point {
        Point::new(
            self.a * p.x + self.c * p.y + self.e,
            self.b * p.x + self.d * p.y + self.f,
        )
    }

    pub fn to_array(&self) -> [f32; 6] {
        [self.a, self.b, self.c, self.d, self.e, self.f]
    }

    /// Rebuilds a `Transform` from the `[a, b, c, d, e, f]` layout produced by [`Transform::to_array`].
    pub fn from_array(m: [f32; 6]) -> Transform {
        Transform {
            a: m[0],
            b: m[1],
            c: m[2],
            d: m[3],
            e: m[4],
            f: m[5],
        }
    }

    /// Returns the affine inverse, or `None` when the linear part is singular (determinant ≈ 0).
    pub fn invert(&self) -> Option<Transform> {
        let det = self.a * self.d - self.b * self.c;
        if det.abs() < 1e-6 {
            return None;
        }
        Some(Transform {
            a: self.d / det,
            b: -self.b / det,
            c: -self.c / det,
            d: self.a / det,
            e: (self.c * self.f - self.d * self.e) / det,
            f: (self.b * self.e - self.a * self.f) / det,
        })
    }
}

#[cfg(test)]
#[path = "transform_test.rs"]
mod tests;
