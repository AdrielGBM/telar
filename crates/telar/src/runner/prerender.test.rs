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

#[cfg(unix)]
#[test]
fn a_page_is_never_written_through_something_already_at_its_path() {
    let dir = std::env::temp_dir().join(format!("telar_prerender_new_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let victim = dir.join("victim");
    std::fs::write(&victim, b"keep").unwrap();
    let planted = dir.join("page-0.json");
    std::os::unix::fs::symlink(&victim, &planted).unwrap();

    let through_link = write_new(&planted, b"{}");
    let fresh = write_new(&dir.join("page-1.json"), b"{}");
    let kept = std::fs::read(&victim).unwrap();
    let _ = std::fs::remove_dir_all(&dir);

    assert!(through_link.is_err());
    assert!(fresh.is_ok());
    assert_eq!(kept, b"keep");
}
