use super::*;

fn spelled(locales: &[LocaleTag]) -> Vec<String> {
    locales.iter().map(ToString::to_string).collect()
}

#[test]
fn a_locale_list_keeps_its_order() {
    assert_eq!(
        spelled(&split_language_tags("es-CL,en-US,fr")),
        ["es-CL", "en-US", "fr"]
    );
}

#[test]
fn an_empty_or_undetermined_list_is_empty() {
    assert!(split_language_tags("").is_empty());
    assert!(split_language_tags("und").is_empty());
}

#[test]
fn the_configuration_locale_is_a_tag() {
    assert_eq!(
        configuration_locale(Some("es"), Some("cl"))
            .map(|tag| tag.to_string())
            .as_deref(),
        Some("es-CL")
    );
    assert_eq!(
        configuration_locale(Some("EN"), None)
            .map(|tag| tag.to_string())
            .as_deref(),
        Some("en")
    );
    assert_eq!(
        configuration_locale(Some("pt"), Some(""))
            .map(|tag| tag.to_string())
            .as_deref(),
        Some("pt")
    );
    assert_eq!(configuration_locale(None, Some("US")), None);
    assert_eq!(configuration_locale(Some(""), None), None);
}

#[test]
fn only_removed_animations_count_as_reduced_motion() {
    assert!(animator_scale_reduces_motion(0.0));
    assert!(!animator_scale_reduces_motion(0.5));
    assert!(!animator_scale_reduces_motion(1.0));
    assert!(!animator_scale_reduces_motion(10.0));
}
