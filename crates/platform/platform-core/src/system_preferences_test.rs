use std::collections::HashMap;

use super::*;

fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    move |name| map.get(name).cloned()
}

#[test]
fn a_posix_name_becomes_a_bcp47_tag() {
    assert_eq!(
        posix_locale_to_bcp47("es_CL.UTF-8").as_deref(),
        Some("es-CL")
    );
    assert_eq!(posix_locale_to_bcp47("en_us").as_deref(), Some("en-US"));
    assert_eq!(posix_locale_to_bcp47("fr").as_deref(), Some("fr"));
    assert_eq!(
        posix_locale_to_bcp47("de_DE@euro").as_deref(),
        Some("de-DE")
    );
    assert_eq!(
        posix_locale_to_bcp47("sr_RS@latin").as_deref(),
        Some("sr-Latn-RS")
    );
    assert_eq!(
        posix_locale_to_bcp47("pt_BR.utf8").as_deref(),
        Some("pt-BR")
    );
}

#[test]
fn names_that_carry_no_language_are_unknown() {
    for raw in ["", "C", "POSIX", "C.UTF-8", "posix", "_US", "12_34"] {
        assert_eq!(posix_locale_to_bcp47(raw), None, "{raw:?}");
    }
}

#[test]
fn language_is_a_priority_list_ahead_of_the_effective_locale() {
    let locales = locales_from_env(env(&[("LANGUAGE", "es_CL:es:en"), ("LANG", "en_US.UTF-8")]));
    assert_eq!(locales, ["es-CL", "es", "en", "en-US"]);
}

#[test]
fn lc_all_wins_over_lc_messages_and_lang() {
    let locales = locales_from_env(env(&[
        ("LC_ALL", "de_DE.UTF-8"),
        ("LC_MESSAGES", "fr_FR.UTF-8"),
        ("LANG", "en_US.UTF-8"),
    ]));
    assert_eq!(locales, ["de-DE"]);

    let locales = locales_from_env(env(&[
        ("LC_MESSAGES", "fr_FR.UTF-8"),
        ("LANG", "en_US.UTF-8"),
    ]));
    assert_eq!(locales, ["fr-FR"]);
}

#[test]
fn an_empty_variable_does_not_mask_the_next_one() {
    let locales = locales_from_env(env(&[("LC_ALL", ""), ("LANG", "it_IT.UTF-8")]));
    assert_eq!(locales, ["it-IT"]);
}

#[test]
fn a_c_locale_disables_language_like_gettext_does() {
    let locales = locales_from_env(env(&[("LANGUAGE", "es:en"), ("LANG", "C.UTF-8")]));
    assert!(locales.is_empty());
}

#[test]
fn no_environment_means_no_locales_rather_than_a_default() {
    assert!(locales_from_env(env(&[])).is_empty());
    assert!(locales_from_env(env(&[("LANGUAGE", "es")])).is_empty());
}

#[test]
fn duplicates_keep_their_first_position() {
    let locales = locales_from_env(env(&[("LANGUAGE", "en_US:es::en_US"), ("LANG", "es")]));
    assert_eq!(locales, ["en-US", "es"]);
}

#[test]
fn unknown_is_the_default_for_every_field() {
    let preferences = SystemPreferences::default();
    assert_eq!(preferences.color_scheme, None);
    assert_eq!(preferences.reduced_motion, None);
    assert_eq!(preferences.high_contrast, None);
    assert!(preferences.locales.is_empty());
    assert_eq!(preferences.prefers_dark(), None);
}

#[test]
fn prefers_dark_reads_the_scheme() {
    let dark = SystemPreferences {
        color_scheme: Some(ColorScheme::Dark),
        ..Default::default()
    };
    let light = SystemPreferences {
        color_scheme: Some(ColorScheme::Light),
        ..Default::default()
    };
    assert_eq!(dark.prefers_dark(), Some(true));
    assert_eq!(light.prefers_dark(), Some(false));
}
