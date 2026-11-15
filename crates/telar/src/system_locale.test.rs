use super::*;
use preferences_core::{SystemPreferences, set_system_preferences};

fn prefer(locales: &[&str]) {
    set_system_preferences(SystemPreferences {
        locales: locales
            .iter()
            .map(|tag| crate::LocaleTag::parse(tag).unwrap())
            .collect(),
        ..SystemPreferences::default()
    });
}

#[test]
fn the_locale_follows_the_preferred_list_live() {
    prefer(&[]);
    follow_system_locale(
        [crate::locale_tag!("es"), crate::locale_tag!("en")],
        crate::locale_tag!("es"),
    );
    assert_eq!(i18n_core::current_locale(), Some(crate::locale_tag!("es")));

    prefer(&["en-GB", "es"]);
    assert_eq!(i18n_core::current_locale(), Some(crate::locale_tag!("en")));

    prefer(&["fr"]);
    assert_eq!(i18n_core::current_locale(), Some(crate::locale_tag!("es")));
}

#[test]
fn an_empty_list_leaves_a_chosen_locale_alone() {
    prefer(&[]);
    i18n_core::set_locale(crate::locale_tag!("en"));
    follow_system_locale(
        [crate::locale_tag!("es"), crate::locale_tag!("en")],
        crate::locale_tag!("es"),
    );
    assert_eq!(i18n_core::current_locale(), Some(crate::locale_tag!("en")));
}

#[test]
fn re_calling_replaces_the_previous_follower() {
    prefer(&[]);
    follow_system_locale([crate::locale_tag!("es")], crate::locale_tag!("es"));
    let one_follower = reactive_core::live_effect_count();
    follow_system_locale(
        [crate::locale_tag!("en"), crate::locale_tag!("pt")],
        crate::locale_tag!("en"),
    );
    assert_eq!(
        reactive_core::live_effect_count(),
        one_follower,
        "a leftover follower would keep negotiating against the old list"
    );
    prefer(&["es-CL", "pt"]);
    assert_eq!(i18n_core::current_locale(), Some(crate::locale_tag!("pt")));
}
