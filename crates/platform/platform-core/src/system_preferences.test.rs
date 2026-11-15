use std::collections::HashMap;

use super::*;

#[test]
fn every_colour_scheme_reads_back_from_its_word() {
    for scheme in ColorScheme::ALL {
        assert_eq!(scheme.as_str().parse(), Ok(scheme));
    }
}

#[test]
fn an_unknown_colour_scheme_is_refused_by_name() {
    let error = "drak".parse::<ColorScheme>().unwrap_err();
    assert_eq!(error, UnknownColorScheme("drak".to_string()));
    assert!(error.to_string().contains("`drak`"));
}

fn spelled(locales: &[LocaleTag]) -> Vec<String> {
    locales.iter().map(ToString::to_string).collect()
}

fn posix(raw: &str) -> Option<String> {
    posix_locale_to_bcp47(raw)
        .unwrap()
        .map(|tag| tag.to_string())
}

fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    move |name| map.get(name).cloned()
}

#[test]
fn a_posix_name_becomes_a_bcp47_tag() {
    assert_eq!(posix("es_CL.UTF-8").as_deref(), Some("es-CL"));
    assert_eq!(posix("en_us").as_deref(), Some("en-US"));
    assert_eq!(posix("fr").as_deref(), Some("fr"));
    assert_eq!(posix("de_DE@euro").as_deref(), Some("de-DE"));
    assert_eq!(posix("sr_RS@latin").as_deref(), Some("sr-Latn-RS"));
    assert_eq!(posix("pt_BR.utf8").as_deref(), Some("pt-BR"));
}

#[test]
fn names_that_carry_no_language_are_unknown() {
    for raw in ["", "C", "POSIX", "C.UTF-8", "posix"] {
        assert_eq!(posix_locale_to_bcp47(raw), Ok(None), "{raw:?}");
    }
}

#[test]
fn a_name_that_is_no_locale_is_an_error() {
    for raw in ["_US", "12_34", "en_US_x!"] {
        assert!(posix_locale_to_bcp47(raw).is_err(), "{raw:?}");
    }
}

#[test]
fn a_name_that_is_no_locale_is_left_out_of_the_preferred_ones() {
    let locales = locales_from_env(env(&[("LANGUAGE", "12_34:es"), ("LANG", "en_US.UTF-8")]));
    assert_eq!(spelled(&locales), ["es", "en-US"]);
}

#[test]
fn the_locales_a_platform_reports_are_read_as_tags_and_one_that_is_none_is_left_out() {
    let locales = platform_locales(["pt_br", "not a tag", "en-US", "PT-BR"]);
    assert_eq!(spelled(&locales), ["pt-BR", "en-US"]);
}

#[test]
fn language_is_a_priority_list_ahead_of_the_effective_locale() {
    let locales = locales_from_env(env(&[("LANGUAGE", "es_CL:es:en"), ("LANG", "en_US.UTF-8")]));
    assert_eq!(spelled(&locales), ["es-CL", "es", "en", "en-US"]);
}

#[test]
fn lc_all_wins_over_lc_messages_and_lang() {
    let locales = locales_from_env(env(&[
        ("LC_ALL", "de_DE.UTF-8"),
        ("LC_MESSAGES", "fr_FR.UTF-8"),
        ("LANG", "en_US.UTF-8"),
    ]));
    assert_eq!(spelled(&locales), ["de-DE"]);

    let locales = locales_from_env(env(&[
        ("LC_MESSAGES", "fr_FR.UTF-8"),
        ("LANG", "en_US.UTF-8"),
    ]));
    assert_eq!(spelled(&locales), ["fr-FR"]);
}

#[test]
fn an_empty_variable_does_not_mask_the_next_one() {
    let locales = locales_from_env(env(&[("LC_ALL", ""), ("LANG", "it_IT.UTF-8")]));
    assert_eq!(spelled(&locales), ["it-IT"]);
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
    assert_eq!(spelled(&locales), ["en-US", "es"]);
}

#[test]
fn unknown_is_the_default_for_every_field_and_text_stays_at_its_designed_size() {
    let preferences = SystemPreferences::default();
    assert_eq!(preferences.color_scheme, None);
    assert_eq!(preferences.reduced_motion, None);
    assert_eq!(preferences.high_contrast, None);
    assert!(preferences.locales.is_empty());
    assert_eq!(preferences.text_scale, TextScale::UNSCALED);
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

#[test]
fn a_text_scale_is_a_finite_factor_above_zero() {
    assert_eq!(TextScale::new(2.0).map(TextScale::get), Some(2.0));
    for refused in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert_eq!(TextScale::new(refused), None, "{refused}");
    }
}
