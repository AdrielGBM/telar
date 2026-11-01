use super::*;
use crate::ticker::{has_active, reset, set_scale, tick};
use reactive_core::{dispose_owner, effect, owner_scope};

fn fresh() -> Instant {
    reset();
    set_scale(1.0);
    preferences_core::set_system_preferences(preferences_core::SystemPreferences::default());
    Instant::now()
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

#[test]
fn an_unread_clock_asks_for_no_frames_and_does_not_move() {
    let base = fresh();
    let clock = FrameClock::new();
    assert!(!has_active(), "nothing reads it");
    tick(base);
    tick(base + ms(50));
    assert_eq!(clock.peek(), Duration::ZERO);
    assert!(!has_active());
}

#[test]
fn it_runs_while_read_and_sleeps_when_the_reader_stops() {
    let base = fresh();
    let clock = FrameClock::new();
    let reading = signal(true);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let log = Rc::clone(&seen);
    let watched = clock.clone();
    let scope = owner_scope();
    effect(move || {
        if reading.get() {
            log.borrow_mut().push(watched.get());
        }
    });
    assert!(has_active(), "a reader wants frames");

    tick(base);
    tick(base + ms(16));
    tick(base + ms(32));
    assert_eq!(clock.peek(), ms(32));
    assert_eq!(seen.borrow().last(), Some(&ms(32)));

    reading.set(false);
    assert!(!has_active(), "the reader stopped reading");
    tick(base + ms(48));
    tick(base + ms(64));
    assert_eq!(clock.peek(), ms(32), "no reader, no time");

    reading.set(true);
    tick(base + ms(80));
    assert_eq!(
        clock.peek(),
        ms(32),
        "the first frame back only sets the origin"
    );
    tick(base + ms(96));
    assert_eq!(clock.peek(), ms(48), "the idle gap was not counted");

    let id = scope.id();
    drop(scope);
    dispose_owner(id);
    assert!(!has_active(), "a disposed reader releases the clock");
}

#[test]
fn a_frame_after_a_long_gap_adds_at_most_the_step_cap() {
    let base = fresh();
    let clock = FrameClock::new();
    let watched = clock.clone();
    effect(move || {
        watched.get();
    });
    tick(base);
    tick(base + ms(10));
    tick(base + Duration::from_secs(30));
    assert_eq!(clock.peek(), ms(10) + MAX_FRAME_STEP);
}

#[test]
fn the_time_scale_slows_it() {
    let base = fresh();
    set_scale(0.5);
    let clock = FrameClock::new();
    let watched = clock.clone();
    effect(move || {
        watched.get();
    });
    tick(base);
    tick(base + ms(40));
    assert_eq!(clock.peek(), ms(20));
    set_scale(1.0);
}

#[test]
fn reduced_motion_freezes_it_and_stops_asking_for_frames() {
    let base = fresh();
    let clock = FrameClock::new();
    let watched = clock.clone();
    effect(move || {
        watched.get();
    });
    tick(base);
    tick(base + ms(20));
    assert_eq!(clock.peek(), ms(20));

    preferences_core::set_system_preferences(preferences_core::SystemPreferences {
        reduced_motion: Some(true),
        ..Default::default()
    });
    tick(base + ms(40));
    assert_eq!(clock.peek(), ms(20), "frozen at its reading");
    assert!(!has_active(), "and not asking for frames");
    tick(base + ms(60));
    assert_eq!(clock.peek(), ms(20));

    preferences_core::set_system_preferences(preferences_core::SystemPreferences {
        reduced_motion: Some(false),
        ..Default::default()
    });
    tick(base + ms(80));
    assert!(has_active(), "full motion again");
    tick(base + ms(100));
    assert_eq!(
        clock.peek(),
        ms(40),
        "it resumed from its reading without the paused gap"
    );
}

#[test]
fn declining_reduced_motion_keeps_it_running() {
    let base = fresh();
    crate::follow_reduced_motion(false);
    preferences_core::set_system_preferences(preferences_core::SystemPreferences {
        reduced_motion: Some(true),
        ..Default::default()
    });
    let clock = FrameClock::new();
    let watched = clock.clone();
    effect(move || {
        watched.get();
    });
    tick(base);
    tick(base + ms(20));
    crate::follow_reduced_motion(true);
    assert_eq!(clock.peek(), ms(20));
}

#[test]
fn while_scopes_the_reader_to_a_condition() {
    let base = fresh();
    let on = signal(false);
    let time = use_frame_time_while(move || on.get());
    assert!(!has_active(), "the condition is false");
    tick(base);
    tick(base + ms(16));
    assert_eq!(time.get(), Duration::ZERO);

    on.set(true);
    assert!(has_active());
    tick(base + ms(32));
    tick(base + ms(48));
    assert_eq!(time.get(), ms(16));

    on.set(false);
    assert!(!has_active());
    tick(base + ms(64));
    assert_eq!(time.get(), ms(16), "held while the condition is false");
}

#[test]
fn the_hook_clock_ends_with_its_scope() {
    let base = fresh();
    let scope = owner_scope();
    let time = use_frame_time();
    effect(move || {
        time.get();
    });
    tick(base);
    tick(base + ms(16));
    assert!(has_active());
    assert_eq!(time.peek(), ms(16));
    let id = scope.id();
    drop(scope);
    dispose_owner(id);
    tick(base + ms(32));
    assert!(!has_active());
}

#[test]
fn reset_returns_to_zero() {
    let base = fresh();
    let clock = FrameClock::new();
    let watched = clock.clone();
    effect(move || {
        watched.get();
    });
    tick(base);
    tick(base + ms(30));
    clock.reset();
    assert_eq!(clock.peek(), Duration::ZERO);
    tick(base + ms(40));
    tick(base + ms(50));
    assert_eq!(clock.peek(), ms(10));
}
