use super::*;

#[test]
fn a_chosen_scheme_is_brought_back_and_kept_as_it_changes() {
    services_core::store_preference(SCHEME_KEY, Some("dark"));
    follow_stored_scheme();
    assert_eq!(theme_core::scheme_preference(), SchemePreference::Dark);
    theme_core::set_scheme_preference(SchemePreference::Light);
    assert_eq!(
        services_core::stored_preference(SCHEME_KEY).as_deref(),
        Some("light")
    );
}

// One test, because the store is process-wide and parallel tests on the same key would race.
#[test]
fn a_reduced_motion_override_is_brought_back_and_kept_as_it_changes() {
    services_core::store_preference(REDUCED_MOTION_KEY, Some("sometimes"));
    follow_stored_reduced_motion();
    assert_eq!(
        preferences_core::reduced_motion_override(),
        None,
        "an unreadable word leaves the system in charge"
    );
    assert_eq!(services_core::stored_preference(REDUCED_MOTION_KEY), None);

    services_core::store_preference(REDUCED_MOTION_KEY, Some("true"));
    follow_stored_reduced_motion();
    assert_eq!(preferences_core::reduced_motion_override(), Some(true));
    preferences_core::set_reduced_motion_override(Some(false));
    assert_eq!(
        services_core::stored_preference(REDUCED_MOTION_KEY).as_deref(),
        Some("false")
    );
    preferences_core::set_reduced_motion_override(None);
    assert_eq!(services_core::stored_preference(REDUCED_MOTION_KEY), None);
}
