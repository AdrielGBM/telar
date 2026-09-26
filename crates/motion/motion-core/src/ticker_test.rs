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

struct Probe {
    reducible: bool,
    seen: std::cell::Cell<Option<f32>>,
}

impl Tickable for Probe {
    fn tick(&self, _now: Instant, scale: f32) {
        self.seen.set(Some(scale));
    }
    fn is_settled(&self) -> bool {
        false
    }
    fn reducible(&self) -> bool {
        self.reducible
    }
}

fn probe(reducible: bool) -> std::rc::Rc<Probe> {
    let probe = std::rc::Rc::new(Probe {
        reducible,
        seen: std::cell::Cell::new(None),
    });
    register(
        next_id(),
        std::rc::Rc::downgrade(&probe) as Weak<dyn Tickable>,
    );
    probe
}

fn reduced_motion(reduced: Option<bool>) {
    preferences_core::set_system_preferences(preferences_core::SystemPreferences {
        reduced_motion: reduced,
        ..Default::default()
    });
}

fn scales_seen(reduced: Option<bool>) -> (Option<f32>, Option<f32>) {
    fresh();
    reduced_motion(reduced);
    let decorative = probe(true);
    let momentum = probe(false);
    tick(Instant::now());
    reduced_motion(None);
    (decorative.seen.get(), momentum.seen.get())
}

#[test]
fn reduced_motion_zeroes_the_scale_by_default() {
    set_scale(0.5);
    let (decorative, momentum) = scales_seen(Some(true));
    assert_eq!(decorative, Some(0.0), "the user asked for less motion");
    assert_eq!(
        momentum,
        Some(0.5),
        "momentum keeps the application's scale"
    );
    assert_eq!(scale(), 0.5, "and the application's own scale survives");
    set_scale(1.0);
}

#[test]
fn an_unknown_or_declined_preference_changes_nothing() {
    assert_eq!(scales_seen(None), (Some(1.0), Some(1.0)));
    assert_eq!(scales_seen(Some(false)), (Some(1.0), Some(1.0)));
}

#[test]
fn an_application_can_decline_to_follow_it() {
    follow_reduced_motion(false);
    assert!(!follows_reduced_motion());
    let seen = scales_seen(Some(true));
    follow_reduced_motion(true);
    assert_eq!(seen, (Some(1.0), Some(1.0)));
}
