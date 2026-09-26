use super::*;

#[test]
fn a_uri_needs_a_scheme_and_something_after_it() {
    assert!(Uri::parse("https://example.com").is_some());
    assert!(Uri::parse("mailto:someone@example.com").is_some());
    assert!(Uri::parse("git+ssh://host/repo").is_some());
    assert!(Uri::parse("/projects/telar").is_none());
    assert!(Uri::parse("projects").is_none());
    assert!(Uri::parse("1http://x").is_none());
    assert!(Uri::parse("https:").is_none());
    assert!(Uri::parse("").is_none());
}

#[test]
fn the_scheme_says_whether_it_is_a_web_page() {
    let page = Uri::parse("HTTPS://example.com").unwrap();
    assert_eq!(page.scheme(), "HTTPS");
    assert!(page.is_web());
    assert!(!Uri::parse("mailto:a@b.c").unwrap().is_web());
}

#[test]
fn constructors_build_each_kind() {
    assert_eq!(
        Destination::anchor("contact"),
        Destination::Anchor("contact".into())
    );
    assert_eq!(
        Destination::external("https://example.com").unwrap(),
        Destination::External(Uri::parse("https://example.com").unwrap())
    );
    assert_eq!(
        Destination::from(Location::root().segment("a")),
        Destination::Route(Location::root().segment("a"))
    );
}

#[test]
fn an_external_destination_without_a_scheme_is_refused() {
    assert!(Destination::external("example.com").is_none());
}
