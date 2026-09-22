use std::cell::Cell;
use std::rc::Rc;

use super::*;

fn dark_spanish() -> SystemPreferences {
    SystemPreferences {
        color_scheme: Some(ColorScheme::Dark),
        reduced_motion: Some(false),
        high_contrast: None,
        locales: vec!["es-CL".into(), "en".into()],
    }
}

#[test]
fn everything_is_unknown_until_the_platform_speaks() {
    assert_eq!(system_preferences(), SystemPreferences::default());
}

#[test]
fn a_snapshot_round_trips() {
    set_system_preferences(dark_spanish());
    assert_eq!(system_preferences(), dark_spanish());
    assert_eq!(use_system_preferences(), dark_spanish());
    set_system_preferences(SystemPreferences::default());
    assert_eq!(system_preferences(), SystemPreferences::default());
}

fn count_runs(read: impl Fn() + 'static) -> (Rc<Cell<u32>>, reactive_core::Effect) {
    let runs = Rc::new(Cell::new(0));
    let counter = runs.clone();
    let effect = reactive_core::effect(move || {
        read();
        counter.set(counter.get() + 1);
    });
    (runs, effect)
}

#[test]
fn a_reader_of_one_field_ignores_the_others() {
    set_system_preferences(dark_spanish());
    let (runs, _effect) = count_runs(|| {
        use_reduced_motion();
    });
    assert_eq!(runs.get(), 1);

    set_system_preferences(SystemPreferences {
        color_scheme: Some(ColorScheme::Light),
        locales: vec!["fr".into()],
        ..dark_spanish()
    });
    assert_eq!(runs.get(), 1, "neither changed field was read");

    set_system_preferences(SystemPreferences {
        reduced_motion: Some(true),
        ..dark_spanish()
    });
    assert_eq!(runs.get(), 2);
}

#[test]
fn an_identical_snapshot_notifies_nobody() {
    set_system_preferences(dark_spanish());
    let (runs, _effect) = count_runs(|| {
        use_system_preferences();
    });
    set_system_preferences(dark_spanish());
    assert_eq!(runs.get(), 1);
}

#[test]
fn a_whole_snapshot_is_one_notification() {
    set_system_preferences(SystemPreferences::default());
    let (runs, _effect) = count_runs(|| {
        use_system_preferences();
    });
    set_system_preferences(dark_spanish());
    assert_eq!(runs.get(), 2);
    assert_eq!(use_preferred_locales(), ["es-CL", "en"]);
    assert_eq!(use_color_scheme(), Some(ColorScheme::Dark));
    assert_eq!(use_high_contrast(), None);
}
