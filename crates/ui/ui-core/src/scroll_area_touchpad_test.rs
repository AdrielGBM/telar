use super::tests::make_scroll_area;
use super::*;
use platform_core::ScrollDelta;
use std::time::{Duration, Instant};

fn glided(y: f32) -> Event {
    Event::Scrolled {
        delta: ScrollDelta::Pixels { x: 0.0, y },
        x: 100.0,
        y: 100.0,
    }
}

fn lifted() -> Event {
    Event::ScrollEnded { x: 100.0, y: 100.0 }
}

/// Two fingers moving fast and then leaving: the same gesture a finger makes on a phone, reported the way a touchpad reports it.
fn flick(sa: &mut ScrollArea) {
    for _ in 0..6 {
        sa.on_event(&glided(-30.0));
        std::thread::sleep(Duration::from_millis(8));
    }
}

fn run_frames(count: u32) {
    let start = Instant::now();
    for frame in 1..=count {
        motion_core::tick(start + Duration::from_millis(16 * frame as u64));
    }
}

#[test]
fn a_flick_carries_on_after_the_fingers_leave() {
    let mut sa = make_scroll_area();
    flick(&mut sa);
    let released_at = sa.core.scroll_y.get();
    sa.on_event(&lifted());
    run_frames(20);
    assert!(
        sa.core.scroll_y.get() > released_at,
        "it should have travelled past where the fingers left it: \
         {released_at} -> {}",
        sa.core.scroll_y.get()
    );
}

/// What macOS does: the system sends its own momentum after the fingers lift. Telar's must not be added on top of it.
#[test]
fn a_platform_running_its_own_momentum_takes_the_offset_back() {
    let mut sa = make_scroll_area();
    flick(&mut sa);
    sa.on_event(&lifted());
    run_frames(2);
    sa.on_event(&glided(-5.0));
    let taken_over = sa.core.scroll_y.get();
    run_frames(20);
    assert_eq!(
        sa.core.scroll_y.get(),
        taken_over,
        "the platform's own scroll is in charge now"
    );
}

#[test]
fn fingers_leaving_without_having_moved_carry_nothing() {
    let mut sa = make_scroll_area();
    sa.on_event(&lifted());
    run_frames(20);
    assert_eq!(sa.core.scroll_y.get(), 0.0);
}

/// The speed belongs to whoever the scroll belonged to: an area that ignored it must not let go of it.
#[test]
fn a_gesture_this_area_never_applied_is_not_its_to_carry() {
    let mut sa = make_scroll_area();
    for _ in 0..6 {
        sa.on_event(&Event::Scrolled {
            delta: ScrollDelta::Pixels { x: 0.0, y: -30.0 },
            x: 900.0,
            y: 900.0,
        });
        std::thread::sleep(Duration::from_millis(8));
    }
    sa.on_event(&lifted());
    run_frames(20);
    assert_eq!(
        sa.core.scroll_y.get(),
        0.0,
        "it was never this area's scroll"
    );
}
