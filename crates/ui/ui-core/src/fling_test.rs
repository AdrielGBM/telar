use super::*;
use motion_core::Tickable;
use std::time::Duration;

fn advance(fling: &Fling, from: Instant, steps: u32, step: Duration) -> Instant {
    let mut now = from;
    fling.tick(now, 1.0);
    for _ in 0..steps {
        now += step;
        fling.tick(now, 1.0);
    }
    now
}

#[test]
fn a_gesture_too_slow_to_mean_anything_carries_nothing() {
    let offset = reactive_core::signal(0.0);
    assert!(
        Fling::start(offset, 40.0, (0.0, 1000.0)).is_none(),
        "a gesture below the threshold is not a fling"
    );
}

#[test]
fn a_gesture_against_the_edge_it_is_heading_for_carries_nothing() {
    let offset = reactive_core::signal(1000.0);
    assert!(
        Fling::start(offset, 900.0, (0.0, 1000.0)).is_none(),
        "a fling into the far edge has nowhere to go"
    );
    let offset = reactive_core::signal(0.0);
    assert!(
        Fling::start(offset, -900.0, (0.0, 1000.0)).is_none(),
        "and neither has one into the near edge"
    );
}

#[test]
fn what_was_moving_keeps_moving_and_slows_down() {
    let offset = reactive_core::signal(0.0);
    let fling = Fling::start(offset, 1200.0, (0.0, 10_000.0)).expect("fast enough to carry");
    let start = Instant::now();
    let after_two = {
        advance(&fling, start, 2, Duration::from_millis(16));
        offset.peek()
    };
    assert!(after_two > 0.0, "it should have travelled: {after_two}");

    let now = advance(
        &fling,
        start + Duration::from_millis(32),
        8,
        Duration::from_millis(16),
    );
    let later = offset.peek();
    assert!(later > after_two, "it should still be travelling");

    let first = after_two;
    let second = later - after_two;
    assert!(second < first * 8.0, "it should be decaying, not coasting");

    advance(&fling, now, 120, Duration::from_millis(16));
    assert!(
        fling.is_settled(),
        "two seconds is long past the end of a fling"
    );
}

#[test]
fn meeting_the_end_of_the_content_stops_it() {
    let offset = reactive_core::signal(90.0);
    let fling = Fling::start(offset, 2000.0, (0.0, 100.0)).expect("fast enough to carry");
    advance(&fling, Instant::now(), 6, Duration::from_millis(16));
    assert_eq!(offset.peek(), 100.0, "it stops at the bound, not past it");
    assert!(
        fling.is_settled(),
        "reaching the end of the content ends the fling"
    );
}

#[test]
fn a_hand_on_the_screen_stops_it_where_it_stands() {
    let offset = reactive_core::signal(0.0);
    let fling = Fling::start(offset, 1500.0, (0.0, 10_000.0)).expect("fast enough to carry");
    let now = advance(&fling, Instant::now(), 3, Duration::from_millis(16));
    let caught = offset.peek();
    fling.stop();
    advance(&fling, now, 10, Duration::from_millis(16));
    assert_eq!(offset.peek(), caught, "it kept the offset it had reached");
}

#[test]
fn a_pause_mid_gesture_is_not_a_fling() {
    let mut velocity = Velocity::default();
    velocity.record(20.0);
    std::thread::sleep(Duration::from_millis(120));
    velocity.record(1.0);
    assert_eq!(
        velocity.take(),
        0.0,
        "a gesture that paused was let go of, not thrown"
    );
}
