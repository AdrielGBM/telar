use std::time::Duration;

use super::*;
use crate::curve::{spring, tween};
use crate::easing::Easing;
use crate::ticker::{has_active, reset, set_scale, tick};
use geometry_core::Rect;

fn fresh() -> Instant {
    reset();
    set_scale(1.0);
    Instant::now()
}

#[test]
fn tween_reaches_eased_value_at_half_duration() {
    let base = fresh();
    let a = Animated::new(0.0f32, tween(Duration::from_millis(200), Easing::EaseInOut));
    a.retarget(1.0);
    tick(base);
    tick(base + Duration::from_millis(100));
    let expected = 0.0f32.lerp(&1.0, Easing::EaseInOut.apply(0.5));
    assert!(
        (a.get() - expected).abs() < 1e-4,
        "{} != {expected}",
        a.get()
    );
}

#[test]
fn tween_settles_at_target_and_goes_inactive() {
    let base = fresh();
    let a = Animated::new(0.0f32, tween(Duration::from_millis(200), Easing::Linear));
    a.retarget(1.0);
    tick(base);
    tick(base + Duration::from_millis(200));
    assert!((a.get() - 1.0).abs() < 1e-6, "{}", a.get());
    assert!(a.is_settled(), "reaching the target is what settling means");
    assert!(!has_active(), "a settled tween leaves the ticker");
}

#[test]
fn first_tick_does_not_move_the_value() {
    let base = fresh();
    let a = Animated::new(0.0f32, tween(Duration::from_millis(200), Easing::Linear));
    a.retarget(1.0);
    tick(base);
    assert_eq!(a.get(), 0.0);
    assert!(
        has_active(),
        "the first tick only establishes t0, it does not end the animation"
    );
}

#[test]
fn spring_settles_at_target() {
    let base = fresh();
    let a = Animated::new(0.0f32, spring(120.0, 22.0));
    a.retarget(1.0);
    tick(base);
    let mut now = base;
    for _ in 0..1000 {
        now += Duration::from_millis(16);
        tick(now);
        if !has_active() {
            break;
        }
    }
    assert!(!has_active(), "spring never settled");
    assert!((a.get() - 1.0).abs() < 1e-2, "settled at {}", a.get());
}

#[test]
fn spring_preserves_velocity_across_retarget() {
    let base = fresh();
    let a = Animated::new(0.0f32, spring(120.0, 14.0));
    a.retarget(1.0);
    tick(base);
    tick(base + Duration::from_millis(16));
    tick(base + Duration::from_millis(32));
    let value_before = a.get();
    a.retarget(value_before);
    tick(base + Duration::from_millis(33));
    tick(base + Duration::from_millis(37));
    assert!(
        a.get() > value_before,
        "momentum lost: {} !> {value_before}",
        a.get()
    );
}

#[test]
fn retarget_to_current_goal_is_a_noop() {
    let _ = fresh();
    let a = Animated::new(5.0f32, tween(Duration::from_millis(200), Easing::Linear));
    a.retarget(5.0);
    assert!(
        a.is_settled(),
        "retargeting to the value it already holds animates nothing"
    );
    assert!(!has_active(), "so there is nothing to tick");
}

#[test]
fn same_target_retarget_does_not_restart_the_tween() {
    let base = fresh();
    let a = Animated::new(0.0f32, tween(Duration::from_millis(200), Easing::Linear));
    a.retarget(1.0);
    tick(base);
    tick(base + Duration::from_millis(100));
    assert!((a.get() - 0.5).abs() < 1e-4, "{}", a.get());
    a.retarget(1.0);
    tick(base + Duration::from_millis(150));
    assert!((a.get() - 0.75).abs() < 1e-4, "restarted: {}", a.get());
}

#[test]
fn scale_zero_jumps_straight_to_target() {
    let base = fresh();
    set_scale(0.0);
    let a = Animated::new(0.0f32, spring(120.0, 14.0));
    a.retarget(1.0);
    tick(base);
    assert_eq!(a.get(), 1.0);
    assert!(a.is_settled(), "a zero scale settles on arrival");
    assert!(!has_active(), "so nothing stays registered");
    set_scale(1.0);
}

// The magnitude is the point: at these coordinates the settle epsilons are below one f32 ULP.
#[test]
fn spring_on_large_coordinates_stops_being_active() {
    let base = fresh();
    let a = Animated::new(Rect::new(1920.0, 1080.0, 240.0, 64.0), spring(180.0, 26.0));
    a.retarget(Rect::new(2400.0, 1080.0, 240.0, 64.0));
    tick(base);
    let mut now = base;
    for _ in 0..600 {
        now += Duration::from_micros(16_667);
        tick(now);
        if !has_active() {
            break;
        }
    }
    assert!(!has_active(), "spring never deregistered: {:?}", a.get());
    assert!(
        (a.get().x - 2400.0).abs() < 1e-2,
        "settled at {:?}",
        a.get()
    );
}

// The tick pattern a multi-surface app produces: one real frame step, then one per sibling surface microseconds behind it.
#[test]
fn sub_frame_ticks_do_not_consume_elapsed_time() {
    let base = fresh();
    let a = Animated::new(0.0f32, tween(Duration::from_millis(200), Easing::Linear));
    a.retarget(1.0);
    tick(base);
    for extra in 1..7u64 {
        tick(base + Duration::from_micros(extra * 20));
    }
    assert_eq!(a.get(), 0.0, "a sub-frame tick moved the value");
    tick(base + Duration::from_millis(100));
    assert!(
        (a.get() - 0.5).abs() < 1e-3,
        "elapsed time was lost to the sub-frame ticks: {}",
        a.get()
    );
}

/// An animation stops when the scope that made it goes, not when a handle does.
///
/// The registry holds a `Weak` and prunes what it cannot upgrade, which is unchanged — what changed is who holds the strong reference. It used to be the handles, so the last one dropping ended the animation; it is the reactive arena now, so disposing the owner does.
#[test]
fn an_animation_ends_with_the_scope_that_made_it() {
    let base = fresh();
    let scope = reactive_core::owner_scope();
    let owner = scope.id();
    let a = Animated::new(0.0f32, tween(Duration::from_millis(200), Easing::Linear));
    a.retarget(1.0);
    tick(base);
    assert!(has_active(), "the tween is running");

    drop(scope);
    assert!(
        has_active(),
        "a handle going out of scope is not the end of it"
    );

    reactive_core::dispose_owner(owner);
    assert!(!has_active(), "disposing the owner ends the animation");
}

fn run(a: &Animated<f32>, from: Instant, ms: u64, step_ms: u64) -> (Instant, Option<u64>) {
    let mut now = from;
    let mut spent = 0;
    while spent < ms {
        spent += step_ms;
        now += Duration::from_millis(step_ms);
        tick(now);
        if a.is_settled() {
            return (now, Some(spent));
        }
    }
    (now, None)
}

#[test]
fn a_reversed_linear_tween_takes_the_time_it_had_spent() {
    let base = fresh();
    let a = Animated::new(0.0f32, tween(Duration::from_millis(200), Easing::Linear));
    a.retarget(1.0);
    tick(base);
    let turned = base + Duration::from_millis(60);
    tick(turned);
    assert!((a.get() - 0.3).abs() < 1e-4, "{}", a.get());

    a.retarget(0.0);
    let (_, settled_after) = run(&a, turned, 200, 1);
    let took = settled_after.expect("the reversal settles");
    assert!(
        took.abs_diff(60) <= 1,
        "the way back took {took} ms, the way out 60"
    );
    assert_eq!(a.get(), 0.0);
}

#[test]
fn a_reversed_eased_tween_takes_its_share_of_the_distance() {
    let base = fresh();
    let a = Animated::new(0.0f32, tween(Duration::from_millis(200), Easing::EaseOut));
    a.retarget(1.0);
    tick(base);
    let turned = base + Duration::from_millis(40);
    tick(turned);
    let travelled = a.get();

    a.retarget(0.0);
    let (_, settled_after) = run(&a, turned, 400, 1);
    let expected = (200.0 * travelled).ceil() as u64;
    let took = settled_after.expect("the reversal settles");
    assert!(
        took.abs_diff(expected) <= 1,
        "took {took} ms, expected about {expected} ms for {travelled} of the way"
    );
}

#[test]
fn a_tween_nudged_further_takes_the_time_that_was_left() {
    let base = fresh();
    let a = Animated::new(0.0f32, tween(Duration::from_millis(200), Easing::Linear));
    a.retarget(1.0);
    tick(base);
    let nudged = base + Duration::from_millis(100);
    tick(nudged);
    a.retarget(1.1);
    let (_, settled_after) = run(&a, nudged, 400, 1);
    let took = settled_after.expect("the leg settles");
    assert!(
        took.abs_diff(120) <= 1,
        "took {took} ms for 0.6 of a full leg"
    );
}

#[test]
fn a_tween_from_rest_takes_its_full_duration_after_an_earlier_leg() {
    let base = fresh();
    let a = Animated::new(0.0f32, tween(Duration::from_millis(200), Easing::Linear));
    a.retarget(1.0);
    tick(base);
    let (rested, _) = run(&a, base, 200, 10);
    assert!(a.is_settled());

    a.retarget(0.9);
    tick(rested);
    let (_, settled_after) = run(&a, rested, 400, 1);
    let took = settled_after.expect("the leg settles");
    assert!(
        took.abs_diff(200) <= 1,
        "a short leg from rest took {took} ms, not the whole duration"
    );
}

#[test]
fn an_interrupting_retarget_moves_on_the_very_next_frame() {
    let base = fresh();
    let a = Animated::new(0.0f32, tween(Duration::from_millis(200), Easing::Linear));
    a.retarget(1.0);
    tick(base);
    tick(base + Duration::from_millis(100));
    let at_interrupt = a.get();
    a.retarget(0.0);
    assert_eq!(a.get(), at_interrupt, "retargeting never jumps the value");
    tick(base + Duration::from_millis(116));
    assert!(
        a.get() < at_interrupt,
        "the frame after an interruption already moves: {}",
        a.get()
    );
}

#[test]
fn displacing_publishes_at_once_and_eases_back_to_the_target() {
    let base = fresh();
    let a = Animated::new(0.0f32, tween(Duration::from_millis(200), Easing::Linear));
    a.displace(40.0);
    assert_eq!(a.get(), 40.0, "the jump is drawn in the frame it happens");
    tick(base);
    tick(base + Duration::from_millis(100));
    assert!((a.get() - 20.0).abs() < 1e-3, "{}", a.get());
    tick(base + Duration::from_millis(200));
    assert_eq!(a.get(), 0.0);
    assert!(a.is_settled());
}

#[test]
fn displacing_a_spring_keeps_its_velocity() {
    let base = fresh();
    let a = Animated::new(0.0f32, spring(170.0, 26.0));
    a.displace(100.0);
    tick(base);
    let mut now = base;
    for _ in 0..4 {
        now += Duration::from_millis(16);
        tick(now);
    }
    let before = a.get();
    tick(now + Duration::from_millis(16));
    let speed = before - a.get();
    now += Duration::from_millis(16);
    let shifted = a.get();
    a.displace(50.0);
    assert_eq!(a.get(), shifted + 50.0);
    tick(now + Duration::from_millis(16));
    let speed_after = shifted + 50.0 - a.get();
    assert!(
        speed_after > speed * 0.5,
        "momentum lost across the displacement: {speed_after} vs {speed}"
    );
}

#[test]
fn displacing_under_a_zero_time_scale_shows_nothing() {
    let _ = fresh();
    set_scale(0.0);
    let a = Animated::new(0.0f32, spring(170.0, 26.0));
    a.displace(40.0);
    assert_eq!(a.get(), 0.0);
    assert!(a.is_settled());
    assert!(!has_active());
    set_scale(1.0);
}
