//! Logical rects to cell rects, and what colour a paint is at a point.

use geometry_core::Transform;
use geometry_core::{Point, Rect};
use renderer_core::{Color, Gradient, GradientKind, Paint};

use crate::metrics::CellSize;

/// A half-open rectangle of cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellRect {
    pub col0: i32,
    pub row0: i32,
    pub col1: i32,
    pub row1: i32,
}

impl CellRect {
    pub const EMPTY: Self = Self {
        col0: 0,
        row0: 0,
        col1: 0,
        row1: 0,
    };

    /// The cells a logical rect covers. Both edges are rounded independently, never the width — that is
    /// what makes two boxes sharing an edge share a column instead of leaving a seam or overlapping by one.
    pub fn of(rect: Rect, cell: CellSize) -> Self {
        Self {
            col0: cell.col_at(rect.x),
            row0: cell.row_at(rect.y),
            col1: cell.col_at(rect.x + rect.width),
            row1: cell.row_at(rect.y + rect.height),
        }
    }

    pub fn intersect(self, other: Self) -> Self {
        let r = Self {
            col0: self.col0.max(other.col0),
            row0: self.row0.max(other.row0),
            col1: self.col1.min(other.col1),
            row1: self.row1.min(other.row1),
        };
        if r.is_empty() { Self::EMPTY } else { r }
    }

    pub fn is_empty(self) -> bool {
        self.col1 <= self.col0 || self.row1 <= self.row0
    }

    pub fn cols(self) -> u16 {
        (self.col1 - self.col0).max(0) as u16
    }

    pub fn rows(self) -> u16 {
        (self.row1 - self.row0).max(0) as u16
    }
}

/// The colour a paint takes at a point, in the same space the point is given in.
///
/// A gradient's stops live in the space its command was emitted in, so the caller hands over a paint whose
/// geometry has already been mapped through the active matrix — which is exact for any affine transform,
/// and the reason this takes a mapped `Paint` rather than a matrix of its own.
pub fn sample(paint: &Paint, x: f32, y: f32) -> Color {
    match paint {
        Paint::Solid(c) => *c,
        Paint::Gradient(g) => sample_gradient(g, x, y),
    }
}

fn sample_gradient(g: &Gradient, x: f32, y: f32) -> Color {
    let t = match g.kind {
        GradientKind::Linear { start, end } => {
            let dx = end.x - start.x;
            let dy = end.y - start.y;
            let len2 = dx * dx + dy * dy;
            if len2 <= f32::EPSILON {
                0.0
            } else {
                ((x - start.x) * dx + (y - start.y) * dy) / len2
            }
        }
        GradientKind::Radial { center, radius } => {
            if radius <= f32::EPSILON {
                1.0
            } else {
                ((x - center.x).hypot(y - center.y)) / radius
            }
        }
    };
    stop_at(g, t.clamp(0.0, 1.0))
}

fn stop_at(g: &Gradient, t: f32) -> Color {
    let stops = g.stops.active();
    match stops {
        [] => Color::TRANSPARENT,
        [only] => only.color,
        _ => {
            if t <= stops[0].position {
                return stops[0].color;
            }
            for pair in stops.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                if t <= b.position {
                    let span = b.position - a.position;
                    let k = if span <= f32::EPSILON {
                        0.0
                    } else {
                        (t - a.position) / span
                    };
                    return lerp(a.color, b.color, k);
                }
            }
            stops[stops.len() - 1].color
        }
    }
}

fn lerp(a: Color, b: Color, t: f32) -> Color {
    let m = |x: f32, y: f32| x + (y - x) * t;
    Color::rgba(m(a.r, b.r), m(a.g, b.g), m(a.b, b.b), m(a.a, b.a))
}

/// The same paint with its geometry mapped through `matrix`, so it can be sampled in window space.
pub fn mapped(paint: &Paint, matrix: [f32; 6], scale: f32) -> Paint {
    let Paint::Gradient(g) = paint else {
        return *paint;
    };
    let t = Transform::from_array(matrix);
    let kind = match g.kind {
        GradientKind::Linear { start, end } => GradientKind::Linear {
            start: t.apply(start),
            end: t.apply(end),
        },
        GradientKind::Radial { center, radius } => GradientKind::Radial {
            center: t.apply(center),
            radius: radius * scale,
        },
    };
    Paint::Gradient(Gradient {
        kind,
        stops: g.stops,
    })
}

/// The centre of a cell, in logical units — where a paint is sampled for it.
pub fn cell_center(col: i32, row: i32, cell: CellSize) -> Point {
    Point::new(
        (col as f32 + 0.5) * cell.width,
        (row as f32 + 0.5) * cell.height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use renderer_core::GradientStops;

    #[test]
    fn neighbouring_boxes_share_a_column() {
        let cell = CellSize::default();
        let left = CellRect::of(Rect::new(0.0, 0.0, 37.5, 16.0), cell);
        let right = CellRect::of(Rect::new(37.5, 0.0, 37.5, 16.0), cell);
        assert_eq!(left.col1, right.col0);
    }

    #[test]
    fn an_intersection_that_misses_is_empty() {
        let a = CellRect {
            col0: 0,
            row0: 0,
            col1: 4,
            row1: 4,
        };
        let b = CellRect {
            col0: 10,
            row0: 10,
            col1: 14,
            row1: 14,
        };
        assert!(a.intersect(b).is_empty());
    }

    #[test]
    fn a_linear_gradient_runs_from_stop_to_stop() {
        let g = Gradient::linear(
            Point::new(0.0, 0.0),
            Point::new(100.0, 0.0),
            &[(0.0, Color::BLACK), (1.0, Color::WHITE)],
        );
        let paint = Paint::Gradient(g);
        assert_eq!(sample(&paint, 0.0, 0.0), Color::BLACK);
        assert_eq!(sample(&paint, 100.0, 0.0), Color::WHITE);
        let mid = sample(&paint, 50.0, 0.0);
        assert!((mid.r - 0.5).abs() < 0.01, "got {}", mid.r);
    }

    #[test]
    fn a_gradient_clamps_outside_its_span() {
        let g = Gradient::linear(
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            &[(0.0, Color::BLACK), (1.0, Color::WHITE)],
        );
        assert_eq!(sample(&Paint::Gradient(g), -50.0, 0.0), Color::BLACK);
        assert_eq!(sample(&Paint::Gradient(g), 500.0, 0.0), Color::WHITE);
    }

    #[test]
    fn mapping_moves_a_gradient_with_its_shape() {
        let g = Gradient::linear(
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            &[(0.0, Color::BLACK), (1.0, Color::WHITE)],
        );
        let moved = mapped(&Paint::Gradient(g), [1.0, 0.0, 0.0, 1.0, 100.0, 0.0], 1.0);
        assert_eq!(sample(&moved, 100.0, 0.0), Color::BLACK);
        assert_eq!(sample(&moved, 110.0, 0.0), Color::WHITE);
    }

    #[test]
    fn a_single_stop_is_a_solid_colour() {
        let g = Gradient {
            kind: GradientKind::Radial {
                center: Point::new(0.0, 0.0),
                radius: 10.0,
            },
            stops: GradientStops::new(&[(0.5, Color::RED)]),
        };
        assert_eq!(sample(&Paint::Gradient(g), 3.0, 4.0), Color::RED);
    }
}
