use crate::point::Point;

/// A 2D affine transform stored as a 2×3 matrix `[a, b, c, d, e, f]`, mapping a
/// point `(x, y)` to `(a*x + c*y + e, b*x + d*y + f)`. This is the same `[f32; 6]`
/// layout consumed by `RenderNode::transform_with`, so `to_array()` plugs in
/// directly. Compose with [`Transform::then`] instead of multiplying matrices by
/// hand.
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

    /// Returns the transform that applies `self` first and then `next`
    /// (`next ∘ self`), so `a.then(b).apply(p) == b.apply(a.apply(p))`.
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_noop() {
        let p = Point::new(3.0, 4.0);
        assert_eq!(Transform::IDENTITY.apply(p), p);
    }

    #[test]
    fn then_composes_in_application_order() {
        let translate = Transform {
            e: 10.0,
            ..Transform::IDENTITY
        };
        let scale = Transform {
            a: 2.0,
            d: 2.0,
            ..Transform::IDENTITY
        };
        let t = translate.then(scale);
        assert_eq!(t.apply(Point::new(0.0, 0.0)), Point::new(20.0, 0.0));
    }

    #[test]
    fn rotate_around_keeps_center_fixed() {
        let c = Point::new(5.0, 5.0);
        let r = Transform::rotate_around(90.0, c.x, c.y).apply(c);
        assert!((r.x - c.x).abs() < 1e-4 && (r.y - c.y).abs() < 1e-4);
    }

    #[test]
    fn scale_around_keeps_center_fixed() {
        let c = Point::new(7.0, 2.0);
        let s = Transform::scale_around(3.0, 3.0, c.x, c.y).apply(c);
        assert!((s.x - c.x).abs() < 1e-4 && (s.y - c.y).abs() < 1e-4);
    }
}
