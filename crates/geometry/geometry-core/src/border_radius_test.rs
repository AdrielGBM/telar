use super::*;

#[test]
fn border_radius_all_sets_all_corners_equal() {
    let br = BorderRadius::all(8.0);
    assert_eq!(br.top_left, 8.0);
    assert_eq!(br.top_right, 8.0);
    assert_eq!(br.bottom_right, 8.0);
    assert_eq!(br.bottom_left, 8.0);
}

#[test]
fn border_radius_zero_all_corners_are_zero() {
    let br = BorderRadius::zero();
    assert_eq!(br.top_left, 0.0);
    assert_eq!(br.top_right, 0.0);
    assert_eq!(br.bottom_right, 0.0);
    assert_eq!(br.bottom_left, 0.0);
}

#[test]
fn border_radius_zero_is_zero() {
    assert!(BorderRadius::zero().is_zero(), "an all-zero radius is zero");
}

#[test]
fn border_radius_non_zero_is_not_zero() {
    assert!(!BorderRadius::all(1.0).is_zero(), "a uniform radius is not");
}

#[test]
fn border_radius_default_is_zero() {
    assert!(
        BorderRadius::default().is_zero(),
        "the default radius is no radius"
    );
}

#[test]
fn border_radius_partial_non_zero_is_not_zero() {
    let br = BorderRadius {
        top_left: 5.0,
        top_right: 0.0,
        bottom_right: 0.0,
        bottom_left: 0.0,
    };
    assert!(
        !br.is_zero(),
        "one rounded corner is enough to make the radius non-zero: {br:?}"
    );
}

#[test]
fn a_full_corner_fits_to_half_the_shorter_side() {
    let pill = BorderRadius::all(BorderRadius::FULL).fitted(100.0, 40.0);
    assert_eq!(pill, BorderRadius::all(20.0));
    let circle = BorderRadius::all(BorderRadius::FULL).fitted(32.0, 32.0);
    assert_eq!(circle, BorderRadius::all(16.0));
}

#[test]
fn corners_that_fit_are_left_alone() {
    let radius = BorderRadius::all(8.0);
    assert_eq!(radius.fitted(100.0, 40.0), radius);
}

#[test]
fn overlapping_corners_shrink_together_as_css_resolves_them() {
    let radius = BorderRadius {
        top_left: 30.0,
        top_right: 10.0,
        bottom_right: 0.0,
        bottom_left: 0.0,
    };
    let fitted = radius.fitted(20.0, 100.0);
    assert_eq!(fitted.top_left, 15.0);
    assert_eq!(fitted.top_right, 5.0);
}

#[test]
fn a_full_corner_on_one_side_rounds_that_side_alone() {
    let top = BorderRadius {
        top_left: BorderRadius::FULL,
        top_right: BorderRadius::FULL,
        bottom_right: 0.0,
        bottom_left: 0.0,
    };
    let fitted = top.fitted(100.0, 40.0);
    assert_eq!((fitted.top_left, fitted.bottom_left), (40.0, 0.0));
}

#[test]
fn a_negative_corner_is_square() {
    assert_eq!(
        BorderRadius::all(-4.0).fitted(10.0, 10.0),
        BorderRadius::zero()
    );
}
