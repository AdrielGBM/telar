use super::*;

#[test]
fn overlaps_no_clip() {
    assert!(
        overlaps(0.0, 0.0, 10.0, 10.0, None),
        "with no clip every rect is visible"
    );
}

#[test]
fn overlaps_inside_clip() {
    let clip = Rect::new(0.0, 0.0, 100.0, 100.0);
    assert!(
        overlaps(10.0, 10.0, 20.0, 20.0, Some(clip)),
        "the rect sits inside the clip"
    );
}

#[test]
fn overlaps_outside_clip() {
    let clip = Rect::new(0.0, 0.0, 10.0, 10.0);
    assert!(
        !overlaps(20.0, 20.0, 5.0, 5.0, Some(clip)),
        "the rect falls outside the clip entirely"
    );
}

#[test]
fn expand_for_shadow_expands_all_sides() {
    let r = Rect::new(10.0, 10.0, 20.0, 20.0);
    let result = expand_for_shadow(r, 5.0, 2.0, 0.0, 0.0);
    assert!(
        result.x < r.x,
        "a shadow must grow the left edge: {result:?} from {r:?}"
    );
    assert!(
        result.y < r.y,
        "a shadow must grow the top edge: {result:?} from {r:?}"
    );
    assert!(
        result.x + result.width > r.x + r.width,
        "a shadow must grow the right edge: {result:?} from {r:?}"
    );
    assert!(
        result.y + result.height > r.y + r.height,
        "a shadow must grow the bottom edge: {result:?} from {r:?}"
    );
}

#[test]
fn transform_clip_rect_identity() {
    let r = transform_clip_rect(
        Transform::IDENTITY.to_array(),
        Rect::new(10.0, 20.0, 30.0, 40.0),
    );
    assert_eq!(r.x, 10.0);
    assert_eq!(r.y, 20.0);
    assert_eq!(r.width, 30.0);
    assert_eq!(r.height, 40.0);
}

#[test]
fn transform_clip_rect_scale() {
    let scale = [2.0, 0.0, 0.0, 2.0, 0.0, 0.0];
    let r = transform_clip_rect(scale, Rect::new(5.0, 5.0, 10.0, 10.0));
    assert!((r.x - 10.0).abs() < 1e-4, "{r:?}");
    assert!((r.y - 10.0).abs() < 1e-4, "{r:?}");
    assert!((r.width - 20.0).abs() < 1e-4, "{r:?}");
    assert!((r.height - 20.0).abs() < 1e-4, "{r:?}");
}

/// A stroke straddles its path, so the visual rect has to reach half the width past the geometry — as `Line` already does. Without it the damage rect is short by that half, and the outer edge of a stroke that moves is never repainted: it leaves a trail behind it.
#[test]
fn a_stroked_path_reaches_half_its_width_past_its_geometry() {
    use crate::{Color, PathData, PathStyle, Stroke};
    use geometry_core::Point;
    use std::sync::Arc;

    let data = Arc::new(
        PathData::new()
            .move_to(Point::new(10.0, 10.0))
            .line_to(Point::new(40.0, 10.0)),
    );
    let bare = DrawCommand::Path {
        data: Arc::clone(&data),
        style: Arc::new(PathStyle::default()),
    };
    let stroked = DrawCommand::Path {
        data,
        style: Arc::new(PathStyle {
            stroke: Some(Stroke::new(Color::BLACK, 6.0)),
            ..Default::default()
        }),
    };

    let matrix = Transform::IDENTITY.to_array();
    let metrics = FontMetrics::default();
    let bare = command_visual_rect(&bare, matrix, &metrics).expect("a path has bounds");
    let wide = command_visual_rect(&stroked, matrix, &metrics).expect("and so does a stroked one");

    assert_eq!(wide.x, bare.x - 3.0);
    assert_eq!(wide.y, bare.y - 3.0);
    assert_eq!(wide.width, bare.width + 6.0);
    assert_eq!(wide.height, bare.height + 6.0);
}
