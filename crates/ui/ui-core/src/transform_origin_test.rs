use layout_core::Direction;

use super::*;
use crate::context::set_direction;

fn rect() -> Rect {
    Rect::new(100.0, 40.0, 200.0, 80.0)
}

fn apply(m: [f32; 6], x: f32, y: f32) -> (f32, f32) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

#[test]
fn the_default_origin_is_the_centre_and_matches_box_transform() {
    set_direction(Direction::Ltr);
    assert_eq!(TransformOrigin::default(), TransformOrigin::CENTER);
    assert_eq!(
        box_transform_about(rect(), TransformOrigin::CENTER, 30.0, 1.5, 0.5, 4.0, -2.0),
        crate::box_transform(rect(), 30.0, 1.5, 0.5, 4.0, -2.0)
    );
}

#[test]
fn an_identity_transform_is_nothing_whatever_the_origin() {
    assert!(
        box_transform_about(rect(), TransformOrigin::start(), 0.0, 1.0, 1.0, 0.0, 0.0).is_none()
    );
}

#[test]
fn scale_from_the_start_pins_the_left_edge_in_ltr() {
    set_direction(Direction::Ltr);
    let m = box_transform_about(rect(), TransformOrigin::start(), 0.0, 2.0, 2.0, 0.0, 0.0).unwrap();
    assert_eq!(apply(m, 100.0, 80.0), (100.0, 80.0), "the pivot stays put");
    assert_eq!(
        apply(m, 300.0, 80.0),
        (500.0, 80.0),
        "the far edge moves away"
    );
}

#[test]
fn start_and_end_follow_the_writing_direction() {
    set_direction(Direction::Rtl);
    let start =
        box_transform_about(rect(), TransformOrigin::start(), 0.0, 2.0, 2.0, 0.0, 0.0).unwrap();
    assert_eq!(
        apply(start, 300.0, 80.0),
        (300.0, 80.0),
        "start is the right edge in RTL"
    );
    let end = box_transform_about(rect(), TransformOrigin::end(), 0.0, 2.0, 2.0, 0.0, 0.0).unwrap();
    assert_eq!(
        apply(end, 100.0, 80.0),
        (100.0, 80.0),
        "end is the left edge in RTL"
    );
    set_direction(Direction::Ltr);
}

#[test]
fn rotation_pivots_on_the_origin() {
    set_direction(Direction::Ltr);
    let m = box_transform_about(
        rect(),
        TransformOrigin::new(0.0, 0.0),
        90.0,
        1.0,
        1.0,
        0.0,
        0.0,
    )
    .unwrap();
    let (x, y) = apply(m, 100.0, 40.0);
    assert!(
        (x - 100.0).abs() < 1e-3 && (y - 40.0).abs() < 1e-3,
        "the corner is fixed"
    );
    let (x, y) = apply(m, 300.0, 40.0);
    assert!(
        (x - 100.0).abs() < 1e-3 && (y - 240.0).abs() < 1e-3,
        "the far corner swings round it: {x} {y}"
    );
}

#[test]
fn translation_ignores_the_pivot() {
    set_direction(Direction::Ltr);
    let m = box_transform_about(rect(), TransformOrigin::end(), 0.0, 1.0, 1.0, 8.0, -4.0).unwrap();
    assert_eq!(m, [1.0, 0.0, 0.0, 1.0, 8.0, -4.0]);
}
