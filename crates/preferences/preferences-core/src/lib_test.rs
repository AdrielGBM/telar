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

#[test]
fn the_frame_clock_read_answers_without_subscribing() {
    set_system_preferences(SystemPreferences::default());
    let (runs, _effect) = count_runs(|| {
        reduced_motion();
    });
    set_system_preferences(SystemPreferences {
        reduced_motion: Some(true),
        ..SystemPreferences::default()
    });
    assert_eq!(reduced_motion(), Some(true));
    assert_eq!(runs.get(), 1, "a peek must not re-run its caller");
}

fn reduced(system: Option<bool>) -> SystemPreferences {
    SystemPreferences {
        reduced_motion: system,
        ..SystemPreferences::default()
    }
}

#[test]
fn without_an_override_reduced_motion_is_the_systems() {
    set_system_preferences(reduced(Some(true)));
    assert_eq!(reduced_motion_override(), None);
    assert_eq!(use_reduced_motion(), Some(true));
    assert_eq!(reduced_motion(), Some(true));
}

#[test]
fn an_override_wins_over_the_system_until_it_is_cleared() {
    set_system_preferences(reduced(Some(true)));
    set_reduced_motion_override(Some(false));
    assert_eq!(use_reduced_motion(), Some(false));
    assert_eq!(reduced_motion(), Some(false));

    set_system_preferences(reduced(None));
    set_reduced_motion_override(Some(true));
    assert_eq!(
        use_reduced_motion(),
        Some(true),
        "it also answers where the system cannot"
    );

    set_system_preferences(reduced(Some(false)));
    assert_eq!(
        reduced_motion(),
        Some(true),
        "a system change does not undo it"
    );

    set_reduced_motion_override(None);
    assert_eq!(use_reduced_motion(), Some(false));
}

#[test]
fn the_system_snapshot_ignores_the_override() {
    set_system_preferences(reduced(Some(true)));
    set_reduced_motion_override(Some(false));
    assert_eq!(use_system_reduced_motion(), Some(true));
    assert_eq!(system_preferences().reduced_motion, Some(true));
    assert_eq!(use_system_preferences().reduced_motion, Some(true));
}

#[test]
fn a_reader_of_reduced_motion_hears_the_override() {
    set_system_preferences(reduced(Some(false)));
    let (runs, _effect) = count_runs(|| {
        use_reduced_motion();
    });
    set_reduced_motion_override(Some(true));
    assert_eq!(runs.get(), 2);
    set_reduced_motion_override(Some(true));
    assert_eq!(runs.get(), 2, "setting the same override notifies nobody");
}

#[test]
fn a_forced_value_is_not_disturbed_by_the_system() {
    set_reduced_motion_override(Some(true));
    let (runs, _effect) = count_runs(|| {
        use_reduced_motion();
    });
    set_system_preferences(reduced(Some(false)));
    assert_eq!(runs.get(), 1);
}
