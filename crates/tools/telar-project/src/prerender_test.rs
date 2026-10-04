use super::*;

fn at(segments: &[&str], locale: Option<&str>) -> PageLocation {
    PageLocation {
        segments: segments.iter().map(|segment| segment.to_string()).collect(),
        locale: locale.map(str::to_string),
    }
}

#[test]
fn a_page_is_the_index_of_the_directory_its_address_names() {
    assert_eq!(
        PageLocation::root().file(),
        Some(PathBuf::from("index.html"))
    );
    assert_eq!(
        at(&[], Some("es")).file(),
        Some(PathBuf::from("es/index.html"))
    );
    assert_eq!(
        at(&["acts", "2"], Some("en")).file(),
        Some(PathBuf::from("en/acts/2/index.html"))
    );
    assert_eq!(at(&["acts", "2"], Some("en")).depth(), 3);
}

#[test]
fn a_segment_no_path_can_hold_has_no_file() {
    for segment in ["", ".", "..", "a/b", "a\\b"] {
        assert_eq!(at(&[segment], None).file(), None, "{segment:?}");
    }
}

#[test]
fn a_request_reads_back_as_it_was_written() {
    let request = PrerenderRequest {
        out: PathBuf::from("/tmp/page.json"),
        page: PageRequest::At {
            location: at(&["projects"], Some("es")),
        },
        surface: Surface::default(),
        preferences: Preferences {
            color_scheme: Some("dark".to_string()),
            ..Preferences::default()
        },
        base: "/docs/".to_string(),
    };
    let written = serde_json::to_string(&request).unwrap();
    assert!(written.contains("\"kind\":\"at\""), "{written}");
    assert_eq!(
        serde_json::from_str::<PrerenderRequest>(&written).unwrap(),
        request
    );
    let not_found = serde_json::to_string(&PageRequest::NotFound).unwrap();
    assert_eq!(not_found, "{\"kind\":\"not_found\"}");
}

#[test]
fn a_request_from_a_packager_that_names_no_base_is_for_a_site_at_the_root() {
    let written = r#"{"out":"/tmp/page.json","page":{"kind":"not_found"},"surface":{"width":1,"height":1},"preferences":{}}"#;
    let request: PrerenderRequest = serde_json::from_str(written).unwrap();
    assert_eq!(request.base, "/");
}
