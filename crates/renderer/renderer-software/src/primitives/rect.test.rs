use super::*;

/// Both branches of [`build_rect_path`] must refuse a box with no area, and neither used to.
///
/// The square branch looks as though it is covered, which is the trap: `tiny_skia::Rect::from_xywh` rejects a *negative* side, not a zero one, so a flat rect passes it and reaches `push_rect` intact. The rounded branch clamps its radii to zero and draws a bare line. Both hand tiny_skia something it declines to fill and warns about, once per frame, for as long as the box stays flat.
#[test]
fn a_box_with_no_area_has_no_path_whatever_its_corners_are() {
    let flat = Rect {
        x: 4.0,
        y: 8.0,
        width: 120.0,
        height: 0.0,
    };
    let thin = Rect {
        width: 0.0,
        height: 120.0,
        ..flat
    };
    for rect in [flat, thin] {
        for radius in [BorderRadius::zero(), BorderRadius::all(8.0)] {
            assert!(
                build_rect_path(rect, radius).is_none(),
                "{rect:?} with {radius:?} still built a path"
            );
        }
    }
}

/// The guard is on the area alone, so a box that has one keeps its rounded path.
#[test]
fn a_box_with_area_still_rounds() {
    let rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 64.0,
        height: 64.0,
    };
    assert!(
        build_rect_path(rect, BorderRadius::all(8.0)).is_some(),
        "a box with area and a radius rounds"
    );
}
