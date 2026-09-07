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
