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

#[test]
fn steps_jump_end_holds_until_the_interval_completes() {
    let steps = Easing::Steps(4, StepPosition::JumpEnd);
    assert_eq!(steps.apply(0.0), 0.0, "jump-end starts flat");
    assert_eq!(steps.apply(0.24), 0.0);
    assert_eq!(steps.apply(0.25), 0.25, "the first interval just completed");
    assert_eq!(steps.apply(0.99), 0.75);
    assert_eq!(steps.apply(1.0), 1.0);
}

#[test]
fn steps_jump_start_jumps_immediately() {
    let steps = Easing::Steps(4, StepPosition::JumpStart);
    assert_eq!(steps.apply(0.0), 0.25, "jump-start already stepped at t=0");
    assert_eq!(steps.apply(0.24), 0.25);
    assert_eq!(steps.apply(0.26), 0.5);
    assert_eq!(steps.apply(1.0), 1.0);
}

#[test]
fn steps_jump_none_holds_flat_at_both_ends() {
    let steps = Easing::Steps(5, StepPosition::JumpNone);
    assert_eq!(steps.apply(0.0), 0.0);
    assert_eq!(steps.apply(0.19), 0.0);
    assert!(
        (steps.apply(0.21) - 0.25).abs() < 1e-6,
        "{}",
        steps.apply(0.21)
    );
    assert_eq!(steps.apply(1.0), 1.0);
}

#[test]
fn steps_jump_both_adds_a_plateau_at_each_end() {
    let steps = Easing::Steps(3, StepPosition::JumpBoth);
    assert_eq!(steps.apply(0.0), 0.25, "jump-both steps once before t=0");
    assert_eq!(steps.apply(1.0), 1.0);
}

#[test]
fn step_position_default_is_jump_end() {
    assert_eq!(StepPosition::default(), StepPosition::JumpEnd);
}
