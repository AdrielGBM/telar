use super::*;
use preferences_core::{SystemPreferences, set_system_preferences};

fn reset() {
    if let Some(previous) = FOLLOW.take() {
        reactive_core::dispose_owner(previous);
    }
    ACTIVE_MODE.with(|s| s.set(None));
    MODES.with(|m| m.borrow_mut().clear());
    SCHEME_PAIR.with(|p| *p.borrow_mut() = None);
    set_system_preferences(SystemPreferences::default());
}

fn scheme(color_scheme: Option<ColorScheme>) {
    set_system_preferences(SystemPreferences {
        color_scheme,
        ..SystemPreferences::default()
    });
}

#[test]
fn set_mode_runs_apply_and_publishes_id() {
    reset();
    let hits = Rc::new(Cell::new(0));
    let h = hits.clone();
    register_mode("dark", move || h.set(h.get() + 1));
    set_mode("dark");
    assert_eq!(hits.get(), 1, "apply closure ran once");
    assert_eq!(active_mode().as_deref(), Some("dark"));
}

#[test]
fn set_mode_publishes_even_without_registration() {
    reset();
    set_mode("unregistered");
    assert_eq!(active_mode().as_deref(), Some("unregistered"));
}

#[test]
fn follow_system_drives_mode_from_the_color_scheme() {
    reset();
    register_mode("day", || {});
    register_mode("night", || {});
    follow_system("day", "night");
    assert_eq!(
        active_mode().as_deref(),
        Some("day"),
        "nothing active and nothing known opens on the light mode"
    );
    scheme(Some(ColorScheme::Dark));
    assert_eq!(active_mode().as_deref(), Some("night"));
    scheme(Some(ColorScheme::Light));
    assert_eq!(active_mode().as_deref(), Some("day"));
}

#[test]
fn an_unknown_scheme_leaves_an_active_mode_alone() {
    reset();
    set_mode("pastel");
    follow_system("day", "night");
    assert_eq!(active_mode().as_deref(), Some("pastel"));
    scheme(Some(ColorScheme::Dark));
    assert_eq!(active_mode().as_deref(), Some("night"));
    scheme(None);
    assert_eq!(
        active_mode().as_deref(),
        Some("night"),
        "losing the answer is not the user asking for light"
    );
}

#[test]
fn a_manual_mode_wins_until_the_scheme_changes() {
    reset();
    follow_system("day", "night");
    set_mode("pastel");
    scheme(None);
    assert_eq!(active_mode().as_deref(), Some("pastel"));
    scheme(Some(ColorScheme::Dark));
    assert_eq!(active_mode().as_deref(), Some("night"));
}

#[test]
fn re_calling_follow_system_replaces_the_previous_follower() {
    reset();
    follow_system("day", "night");
    let one_follower = reactive_core::live_effect_count();
    follow_system("light", "dark");
    assert_eq!(
        reactive_core::live_effect_count(),
        one_follower,
        "a leftover follower would keep setting the old pair's modes"
    );
    let old_pair = Rc::new(Cell::new(0));
    let counter = old_pair.clone();
    register_mode("night", move || counter.set(counter.get() + 1));
    scheme(Some(ColorScheme::Dark));
    assert_eq!(active_mode().as_deref(), Some("dark"));
    assert_eq!(old_pair.get(), 0);
}

#[test]
fn is_dark_false_for_unpaired_third_mode() {
    reset();
    set_light_dark("day", "night");
    set_mode("pastel");
    assert!(
        !is_dark(),
        "a third mode outside the pair is neither dark nor light"
    );
}

#[test]
fn use_mode_is_reactive() {
    reset();
    let seen = Rc::new(RefCell::new(Vec::<Option<String>>::new()));
    let s = seen.clone();
    let _e = reactive_core::effect(move || s.borrow_mut().push(use_mode()));
    set_mode("a");
    set_mode("b");
    let got = seen.borrow().clone();
    assert_eq!(
        got,
        vec![None, Some("a".into()), Some("b".into())],
        "effect re-ran on each mode switch"
    );
}
