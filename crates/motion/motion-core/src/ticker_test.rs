use super::*;

fn fresh() {
    reset();
}

#[test]
fn a_live_guard_keeps_the_loop_awake() {
    fresh();
    assert!(!has_continuous(), "nothing is asking for frames yet");
    let region = Continuous::new();
    assert!(has_continuous(), "a live guard keeps the loop awake");
    drop(region);
    assert!(!has_continuous(), "and dropping it lets the loop sleep");
}

#[test]
fn the_loop_sleeps_only_when_the_last_region_goes() {
    fresh();
    let first = Continuous::new();
    let second = Continuous::new();
    drop(first);
    assert!(has_continuous(), "one region is still on screen");
    drop(second);
    assert!(
        !has_continuous(),
        "the loop sleeps only once the last region goes"
    );
}

// A guard lives in the tree a reload tears down, so its Drop runs against a counter already cleared. Saturating there rather than wrapping is what keeps a reload from leaving a phantom region behind.
#[test]
fn a_reload_leaves_no_phantom_region_scheduling_frames() {
    fresh();
    let region = Continuous::new();
    reset();
    assert!(
        !has_continuous(),
        "a reload must leave no region behind scheduling frames"
    );
    drop(region);
    assert!(!has_continuous(), "the counter must not wrap below zero");
}

// Animations settle and continuous regions do not; the runner asks the two questions separately because only the second one has to move the content generation.
#[test]
fn a_continuous_region_is_not_an_active_animation() {
    fresh();
    let _region = Continuous::new();
    assert!(has_continuous(), "a continuous region keeps frames coming");
    assert!(!has_active(), "no animation was registered");
}
