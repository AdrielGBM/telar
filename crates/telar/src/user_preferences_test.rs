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
