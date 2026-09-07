use super::*;

#[test]
fn linear_is_identity() {
    assert!(
        (Easing::Linear.apply(0.3) - 0.3).abs() < 1e-6,
        "{}",
        Easing::Linear.apply(0.3)
    );
}

#[test]
fn apply_clamps_out_of_range_input() {
    assert_eq!(Easing::Linear.apply(-1.0), 0.0);
    assert_eq!(Easing::Linear.apply(2.0), 1.0);
}

#[test]
fn cubic_easings_hit_endpoints() {
    for easing in [Easing::EaseIn, Easing::EaseOut, Easing::EaseInOut] {
        assert!(easing.apply(0.0).abs() < 1e-6, "{easing:?} at 0");
        assert!((easing.apply(1.0) - 1.0).abs() < 1e-6, "{easing:?} at 1");
    }
}

#[test]
fn ease_in_out_is_symmetric_at_midpoint() {
    assert!(
        (Easing::EaseInOut.apply(0.5) - 0.5).abs() < 1e-6,
        "{}",
        Easing::EaseInOut.apply(0.5)
    );
}

#[test]
fn cubic_bezier_diagonal_is_linear() {
    let curve = Easing::CubicBezier(0.0, 0.0, 1.0, 1.0);
    for &t in &[0.0, 0.25, 0.5, 0.75, 1.0] {
        assert!((curve.apply(t) - t).abs() < 1e-3, "t={t}");
    }
}

#[test]
fn cubic_bezier_css_ease_accelerates_early() {
    // CSS `ease` = cubic-bezier(0.25, 0.1, 0.25, 1.0); at t=0.5 it is well past halfway (~0.8).
    let ease = Easing::CubicBezier(0.25, 0.1, 0.25, 1.0);
    assert!(
        ease.apply(0.0).abs() < 1e-4,
        "a curve must start at 0: {}",
        ease.apply(0.0)
    );
    assert!(
        (ease.apply(1.0) - 1.0).abs() < 1e-4,
        "and end at 1: {}",
        ease.apply(1.0)
    );
    let mid = ease.apply(0.5);
    assert!(mid > 0.5, "ease at 0.5 = {mid}");
}

#[test]
fn cubic_bezier_is_monotonic() {
    let curve = Easing::CubicBezier(0.42, 0.0, 0.58, 1.0);
    let mut prev = curve.apply(0.0);
    for i in 1..=20 {
        let y = curve.apply(i as f32 / 20.0);
        assert!(y >= prev - 1e-4, "not monotonic at {i}: {y} < {prev}");
        prev = y;
    }
}
