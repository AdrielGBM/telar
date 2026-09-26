use super::*;

const SHIPPED: [&str; 3] = ["es", "en", "pt-BR"];

#[test]
fn an_exact_tag_wins_and_comes_back_as_the_application_spells_it() {
    assert_eq!(negotiate_locale(&["PT-br"], &SHIPPED, "es"), "pt-BR");
}

#[test]
fn a_regional_preference_falls_back_to_its_language() {
    assert_eq!(negotiate_locale(&["es-CL"], &SHIPPED, "en"), "es");
    assert_eq!(negotiate_locale(&["pt-PT"], &SHIPPED, "es"), "pt-BR");
}

#[test]
fn a_bare_language_accepts_a_regional_tag() {
    assert_eq!(negotiate_locale(&["pt"], &SHIPPED, "es"), "pt-BR");
}

#[test]
fn among_several_of_one_language_the_first_available_wins() {
    let shipped = ["es-ES", "es-MX", "es"];
    assert_eq!(negotiate_locale(&["es-CL"], &shipped, "es"), "es-ES");
}

#[test]
fn the_order_of_preference_outranks_the_quality_of_the_match() {
    assert_eq!(
        negotiate_locale(&["en-US", "es"], &["es", "en-GB"], "es"),
        "en-GB",
        "English was asked for first, so a British catalog beats an exact Spanish one"
    );
}

#[test]
fn nothing_in_common_or_nothing_asked_is_the_fallback() {
    assert_eq!(negotiate_locale(&["fr", "de"], &SHIPPED, "es"), "es");
    assert_eq!(negotiate_locale(&[] as &[&str], &SHIPPED, "es"), "es");
    assert_eq!(negotiate_locale(&["es"], &[] as &[&str], "en"), "en");
}

#[test]
fn an_empty_preference_matches_nothing() {
    assert_eq!(negotiate_locale(&["", "en"], &SHIPPED, "es"), "en");
}

#[test]
fn owned_lists_work_as_well_as_borrowed_ones() {
    let preferred: Vec<String> = vec!["en-AU".into()];
    let shipped: Vec<String> = vec!["es".into(), "en".into()];
    assert_eq!(negotiate_locale(&preferred, &shipped, "es"), "en");
}
