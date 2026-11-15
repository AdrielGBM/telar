use super::*;

fn tags(texts: &[&str]) -> Vec<LocaleTag> {
    texts
        .iter()
        .map(|text| LocaleTag::parse(text).unwrap())
        .collect()
}

fn negotiate(preferred: &[&str], available: &[&str]) -> Option<String> {
    negotiate_locale(&tags(preferred), &tags(available)).map(ToString::to_string)
}

const SHIPPED: [&str; 3] = ["es", "en", "pt-BR"];

#[test]
fn an_exact_tag_wins_whatever_its_spelling() {
    assert_eq!(negotiate(&["PT-br"], &SHIPPED), Some("pt-BR".into()));
    assert_eq!(negotiate(&["pt_BR"], &SHIPPED), Some("pt-BR".into()));
}

#[test]
fn a_regional_preference_falls_back_to_its_language() {
    assert_eq!(negotiate(&["es-CL"], &SHIPPED), Some("es".into()));
    assert_eq!(negotiate(&["pt-PT"], &SHIPPED), Some("pt-BR".into()));
}

#[test]
fn a_bare_language_accepts_a_regional_tag() {
    assert_eq!(negotiate(&["pt"], &SHIPPED), Some("pt-BR".into()));
}

#[test]
fn among_several_of_one_language_the_first_available_wins() {
    assert_eq!(
        negotiate(&["es-CL"], &["es-ES", "es-MX", "es"]),
        Some("es-ES".into())
    );
}

#[test]
fn the_order_of_preference_outranks_the_quality_of_the_match() {
    assert_eq!(
        negotiate(&["en-US", "es"], &["es", "en-GB"]),
        Some("en-GB".into()),
        "English was asked for first, so a British catalog beats an exact Spanish one"
    );
}

#[test]
fn nothing_in_common_or_nothing_asked_is_no_answer() {
    assert_eq!(negotiate(&["fr", "de"], &SHIPPED), None);
    assert_eq!(negotiate(&[], &SHIPPED), None);
    assert_eq!(negotiate(&["es"], &[]), None);
}
