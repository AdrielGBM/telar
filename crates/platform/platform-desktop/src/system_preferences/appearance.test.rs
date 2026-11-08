use platform_core::{ColorScheme, SystemPreferences};

use super::*;

#[test]
fn color_scheme_follows_the_portal_spec() {
    assert_eq!(
        AppearanceSetting::parse("color-scheme", 1),
        Some(AppearanceSetting::ColorScheme(Some(ColorScheme::Dark)))
    );
    assert_eq!(
        AppearanceSetting::parse("color-scheme", 2),
        Some(AppearanceSetting::ColorScheme(Some(ColorScheme::Light)))
    );
}

#[test]
fn no_color_preference_is_unknown_not_light() {
    assert_eq!(
        AppearanceSetting::parse("color-scheme", 0),
        Some(AppearanceSetting::ColorScheme(None))
    );
}

#[test]
fn contrast_and_motion_are_flags_where_zero_is_a_real_no() {
    assert_eq!(
        AppearanceSetting::parse("contrast", 1),
        Some(AppearanceSetting::HighContrast(Some(true)))
    );
    assert_eq!(
        AppearanceSetting::parse("contrast", 0),
        Some(AppearanceSetting::HighContrast(Some(false)))
    );
    assert_eq!(
        AppearanceSetting::parse("reduced-motion", 1),
        Some(AppearanceSetting::ReducedMotion(Some(true)))
    );
    assert_eq!(
        AppearanceSetting::parse("reduced-motion", 0),
        Some(AppearanceSetting::ReducedMotion(Some(false)))
    );
}

#[test]
fn a_value_outside_the_spec_is_unknown() {
    assert_eq!(
        AppearanceSetting::parse("color-scheme", 7),
        Some(AppearanceSetting::ColorScheme(None))
    );
    assert_eq!(
        AppearanceSetting::parse("contrast", 2),
        Some(AppearanceSetting::HighContrast(None))
    );
}

#[test]
fn keys_outside_the_model_are_ignored() {
    assert_eq!(AppearanceSetting::parse("accent-color", 1), None);
    assert_eq!(AppearanceSetting::parse("", 1), None);
}

#[test]
fn a_portal_that_predates_a_key_leaves_that_field_unknown() {
    let preferences = from_settings([("color-scheme", 1)]);
    assert_eq!(
        preferences,
        SystemPreferences {
            color_scheme: Some(ColorScheme::Dark),
            ..SystemPreferences::default()
        }
    );
}

#[test]
fn a_change_signal_updates_only_its_own_field() {
    let mut preferences =
        from_settings([("color-scheme", 2), ("contrast", 0), ("reduced-motion", 0)]);
    AppearanceSetting::parse("reduced-motion", 1)
        .unwrap()
        .apply(&mut preferences);
    assert_eq!(preferences.color_scheme, Some(ColorScheme::Light));
    assert_eq!(preferences.high_contrast, Some(false));
    assert_eq!(preferences.reduced_motion, Some(true));
}
