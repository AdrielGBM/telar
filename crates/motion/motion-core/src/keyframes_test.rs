use super::*;
use crate::curve::{spring, tween};
use crate::ticker::{has_active, reset, set_scale, tick};
use crate::{Animated, Easing};

fn fresh() -> Instant {
    reset();
    set_scale(1.0);
    Instant::now()
}

// Regression: restart()'s signal set flushes synchronously at batch depth 0, re-running any subscribed effect that calls get() — which must not hit a live RefCell borrow (the sandbox Replay button panicked here).
#[test]
fn restart_with_subscribed_effect_does_not_reentrantly_panic() {
    use std::cell::Cell;
    use std::rc::Rc;
    let base = fresh();
    let kf = Keyframes::new(0.0f32)
        .then(10.0, Duration::from_millis(100), Easing::Linear)
        .start(Repeat::Once);
    let seen = Rc::new(Cell::new(-1.0f32));
    let seen_c = Rc::clone(&seen);
    let kf_read = kf.clone();
    let _e = reactive_core::effect(move || seen_c.set(kf_read.get()));
    tick(base);
    tick(base + Duration::from_millis(200));
    assert!(kf.is_finished(), "200ms is past the only 100ms step");
    assert_eq!(seen.get(), 10.0);

    kf.restart();
    assert_eq!(
        seen.get(),
        0.0,
        "effect observes the reset value during restart's flush"
    );
    assert!(has_active(), "restart re-registers with the ticker");
    tick(base + Duration::from_millis(300));
    tick(base + Duration::from_millis(350));
    assert_eq!(seen.get(), 5.0, "sequence replays after restart");
}

#[test]
fn once_respects_easing_mid_step_then_chains_then_holds_then_settles() {
    let base = fresh();
    let kf = Keyframes::new(0.0f32)
        .then(1.0, Duration::from_millis(200), Easing::EaseInOut)
        .then(2.0, Duration::from_millis(100), Easing::Linear)
        .hold(Duration::from_millis(50))
        .start(Repeat::Once);
    tick(base); // establishes t0, no movement yet
    assert_eq!(kf.get(), 0.0);

    tick(base + Duration::from_millis(100));
    let expected = 0.0f32.lerp(&1.0, Easing::EaseInOut.apply(0.5));
    assert!((kf.get() - expected).abs() < 1e-4, "{}", kf.get());

    tick(base + Duration::from_millis(250));
    assert!((kf.get() - 1.5).abs() < 1e-4, "{}", kf.get());

    tick(base + Duration::from_millis(320));
    assert!((kf.get() - 2.0).abs() < 1e-4, "{}", kf.get());
    assert!(has_active(), "the 50ms hold is still running");
    assert!(!kf.is_finished(), "a hold is not completion");

    tick(base + Duration::from_millis(400));
    assert!((kf.get() - 2.0).abs() < 1e-6, "{}", kf.get());
    assert!(
        kf.is_finished(),
        "the hold elapsed, so the sequence is done"
    );
    assert!(!has_active(), "a finished Once deregisters itself");
}

#[test]
fn loop_wraps_with_a_discrete_jump_and_stays_active() {
    let base = fresh();
    let kf = Keyframes::new(0.0f32)
        .then(1.0, Duration::from_millis(100), Easing::Linear)
        .start(Repeat::Loop);
    tick(base);
    tick(base + Duration::from_millis(250));
    assert!((kf.get() - 0.5).abs() < 1e-4, "{}", kf.get());
    assert!(has_active(), "Loop must stay registered indefinitely");
}

#[test]
fn pingpong_reverses_and_decreases_on_the_way_back() {
    let base = fresh();
    let kf = Keyframes::new(0.0f32)
        .then(1.0, Duration::from_millis(100), Easing::Linear)
        .start(Repeat::PingPong);
    tick(base);
    tick(base + Duration::from_millis(100));
    assert!((kf.get() - 1.0).abs() < 1e-4, "{}", kf.get());
    tick(base + Duration::from_millis(120));
    assert!(kf.get() < 1.0, "did not decrease: {}", kf.get());
    assert!((kf.get() - 0.8).abs() < 1e-4, "{}", kf.get());
    assert!(has_active(), "PingPong must stay registered indefinitely");
}

#[test]
fn restart_after_finished_resets_and_reregisters() {
    let base = fresh();
    let kf = Keyframes::new(0.0f32)
        .then(1.0, Duration::from_millis(100), Easing::Linear)
        .start(Repeat::Once);
    tick(base);
    tick(base + Duration::from_millis(100));
    assert!(kf.is_finished(), "the single 100ms step is over");
    assert!(!has_active(), "and nothing is left to tick");

    kf.restart();
    assert_eq!(kf.get(), 0.0);
    assert!(!kf.is_finished(), "restart clears the finished flag");
    assert!(has_active(), "and puts it back on the ticker");

    tick(base + Duration::from_millis(200)); // re-establishes t0 after restart
    tick(base + Duration::from_millis(250));
    assert!((kf.get() - 0.5).abs() < 1e-4, "{}", kf.get());
}

#[test]
fn stop_deregisters_and_freezes_without_marking_finished() {
    let base = fresh();
    let kf = Keyframes::new(0.0f32)
        .then(1.0, Duration::from_millis(200), Easing::Linear)
        .start(Repeat::Once);
    tick(base);
    tick(base + Duration::from_millis(100));
    assert!((kf.get() - 0.5).abs() < 1e-4, "{}", kf.get());

    kf.stop();
    assert!(!has_active(), "stop() deregisters immediately");
    assert!(!kf.is_finished(), "stop() is not natural completion");
    assert!((kf.get() - 0.5).abs() < 1e-4, "value moved after stop");
}

#[test]
fn spring_presets_build_expected_values() {
    assert_eq!(crate::Spring::gentle(), spring(120.0, 14.0));
    assert_eq!(crate::Spring::snappy(), spring(210.0, 20.0));
    assert_eq!(crate::Spring::bouncy(), spring(180.0, 12.0));
}

#[test]
fn ticker_tracks_animated_and_keyframes_together() {
    let base = fresh();
    let anim = Animated::new(0.0f32, tween(Duration::from_millis(100), Easing::Linear));
    anim.retarget(1.0);
    let kf = Keyframes::new(0.0f32)
        .then(1.0, Duration::from_millis(100), Easing::Linear)
        .start(Repeat::Loop);

    tick(base);
    assert!(has_active(), "both the tween and the loop are registered");
    tick(base + Duration::from_millis(50));
    assert!((anim.get() - 0.5).abs() < 1e-4, "{}", anim.get());
    assert!((kf.get() - 0.5).abs() < 1e-4, "{}", kf.get());
    assert!(has_active(), "neither has reached its end");

    tick(base + Duration::from_millis(100));
    assert!((anim.get() - 1.0).abs() < 1e-6, "{}", anim.get());
    assert!(anim.is_settled(), "the tween reached its target");
    assert!(has_active(), "Keyframes loop must keep the ticker active");

    kf.stop();
    assert!(
        !has_active(),
        "the tween settled and the loop stopped, so nothing is left"
    );
}
