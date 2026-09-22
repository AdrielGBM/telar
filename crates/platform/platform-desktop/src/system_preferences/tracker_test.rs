use platform_core::{ColorScheme, SystemPreferences};

use super::PreferencesTracker;

fn dark() -> SystemPreferences {
    SystemPreferences {
        color_scheme: Some(ColorScheme::Dark),
        locales: vec!["es-CL".into()],
        ..SystemPreferences::default()
    }
}

#[test]
fn the_first_snapshot_is_always_news() {
    let mut tracker = PreferencesTracker::default();
    assert_eq!(
        tracker.update(SystemPreferences::default()),
        Some(SystemPreferences::default())
    );
}

#[test]
fn an_identical_reread_is_not_an_event() {
    let mut tracker = PreferencesTracker::default();
    tracker.update(dark());
    assert_eq!(tracker.update(dark()), None);
}

#[test]
fn a_changed_field_delivers_the_whole_snapshot() {
    let mut tracker = PreferencesTracker::default();
    tracker.update(dark());
    let next = SystemPreferences {
        reduced_motion: Some(true),
        ..dark()
    };
    assert_eq!(tracker.update(next.clone()), Some(next));
}

#[test]
fn a_single_key_changes_only_its_field() {
    let mut tracker = PreferencesTracker::default();
    tracker.update(dark());
    let next = tracker
        .modify(|preferences| preferences.high_contrast = Some(true))
        .expect("the key changed something");
    assert_eq!(next.color_scheme, Some(ColorScheme::Dark));
    assert_eq!(next.locales, ["es-CL"]);
    assert_eq!(next.high_contrast, Some(true));
    assert_eq!(
        tracker.modify(|preferences| preferences.high_contrast = Some(true)),
        None
    );
}

#[test]
fn a_key_before_any_read_waits_for_the_read() {
    let mut tracker = PreferencesTracker::default();
    assert_eq!(
        tracker.modify(|preferences| preferences.reduced_motion = Some(true)),
        None
    );
}
