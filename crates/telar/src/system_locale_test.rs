use super::*;
use preferences_core::{SystemPreferences, set_system_preferences};

fn prefer(locales: &[&str]) {
    set_system_preferences(SystemPreferences {
        locales: locales.iter().map(|tag| tag.to_string()).collect(),
        ..SystemPreferences::default()
    });
}

#[test]
fn the_locale_follows_the_preferred_list_live() {
    prefer(&[]);
    follow_system_locale(["es", "en"], "es");
    assert_eq!(i18n_core::current_locale().as_deref(), Some("es"));

    prefer(&["en-GB", "es"]);
    assert_eq!(i18n_core::current_locale().as_deref(), Some("en"));

    prefer(&["fr"]);
    assert_eq!(i18n_core::current_locale().as_deref(), Some("es"));
}

#[test]
fn an_empty_list_leaves_a_chosen_locale_alone() {
    prefer(&[]);
    i18n_core::set_locale("en");
    follow_system_locale(["es", "en"], "es");
    assert_eq!(i18n_core::current_locale().as_deref(), Some("en"));
}

#[test]
fn re_calling_replaces_the_previous_follower() {
    prefer(&[]);
    follow_system_locale(["es"], "es");
    let one_follower = reactive_core::live_effect_count();
    follow_system_locale(["en", "pt"], "en");
    assert_eq!(
        reactive_core::live_effect_count(),
        one_follower,
        "a leftover follower would keep negotiating against the old list"
    );
    prefer(&["es-CL", "pt"]);
    assert_eq!(i18n_core::current_locale().as_deref(), Some("pt"));
}
