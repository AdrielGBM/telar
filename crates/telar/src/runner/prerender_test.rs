use super::*;

#[test]
fn an_assumed_scheme_is_reported_as_the_platform_would() {
    let preferences = Preferences {
        color_scheme: Some("dark".to_string()),
        reduced_motion: Some(true),
        high_contrast: None,
        locales: vec!["es-CL".to_string()],
    };
    let reported = system_preferences(&preferences);
    assert_eq!(reported.color_scheme, Some(ColorScheme::Dark));
    assert_eq!(reported.reduced_motion, Some(true));
    assert_eq!(reported.high_contrast, None);
    assert_eq!(reported.locales, ["es-CL"]);
}

#[test]
fn a_scheme_nobody_knows_is_left_unknown() {
    let preferences = Preferences {
        color_scheme: Some("sepia".to_string()),
        ..Preferences::default()
    };
    assert_eq!(system_preferences(&preferences).color_scheme, None);
}

#[test]
fn a_page_is_opened_at_its_location_in_its_locale() {
    let page = PageRequest::At {
        location: PageLocation {
            segments: vec!["acts".to_string(), "2".to_string()],
            locale: Some("es".to_string()),
        },
    };
    assert_eq!(
        location_of(&page),
        Location::root()
            .segment("acts")
            .segment("2")
            .with_locale("es")
    );
}

#[test]
fn the_page_for_no_page_stands_where_no_route_reads() {
    let location = location_of(&PageRequest::NotFound);
    assert_eq!(location.segments().len(), 1);
    assert!(location.segments()[0].starts_with('\u{0}'));
}
