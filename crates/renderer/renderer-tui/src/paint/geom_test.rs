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
    assert!(
        a.intersect(b).is_empty(),
        "rects that do not touch intersect in nothing: {:?}",
        a.intersect(b)
    );
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
