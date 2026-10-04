//! The point a box's `rotate` and `scale` pivot on.

use geometry_core::{Rect, Transform};

use crate::context::use_direction;

/// Where on a box its rotation and scale are anchored, as fractions of the box measured from its inline-start and block-start corner.
///
/// The horizontal fraction follows the writing direction: `0.0` is the left edge in left-to-right text and the right edge in right-to-left text, so a box that scales "from the start" grows away from the side its text begins on in both. The vertical fraction runs from the top edge down. Fractions outside `0..=1` anchor beyond the box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransformOrigin {
    pub x: f32,
    pub y: f32,
}

impl TransformOrigin {
    pub const CENTER: Self = Self { x: 0.5, y: 0.5 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// The inline-start edge, vertically centred.
    pub const fn start() -> Self {
        Self::new(0.0, 0.5)
    }

    /// The inline-end edge, vertically centred.
    pub const fn end() -> Self {
        Self::new(1.0, 0.5)
    }

    /// The pivot in the surface's coordinates for a box laid out as `rect`, reading the writing direction only when the horizontal fraction is not the centre.
    pub fn resolve(self, rect: Rect) -> (f32, f32) {
        let inline = match self.x == 0.5 {
            true => 0.5,
            false if use_direction().is_rtl() => 1.0 - self.x,
            false => self.x,
        };
        (rect.x + rect.width * inline, rect.y + rect.height * self.y)
    }
}

impl Default for TransformOrigin {
    fn default() -> Self {
        Self::CENTER
    }
}

/// [`box_transform`](crate::box_transform) with the rotation and scale pivoting on `origin` instead of the box centre. Translation is unaffected by the pivot.
pub fn box_transform_about(
    rect: Rect,
    origin: TransformOrigin,
    rotate_deg: f32,
    scale_x: f32,
    scale_y: f32,
    translate_x: f32,
    translate_y: f32,
) -> Option<[f32; 6]> {
    if rotate_deg == 0.0
        && scale_x == 1.0
        && scale_y == 1.0
        && translate_x == 0.0
        && translate_y == 0.0
    {
        return None;
    }
    let (px, py) = origin.resolve(rect);
    let matrix = Transform::rotate_around(rotate_deg, px, py)
        .then(Transform::scale_around(scale_x, scale_y, px, py))
        .then(Transform::translate(translate_x, translate_y));
    Some(matrix.to_array())
}

#[cfg(test)]
#[path = "transform_origin_test.rs"]
mod tests;
