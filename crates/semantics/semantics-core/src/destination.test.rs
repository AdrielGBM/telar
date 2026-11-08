use super::*;

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
fn an_external_destination_is_refused_unless_the_policy_allows_it() {
    assert_eq!(
        Destination::external("example.com"),
        Err(UriError::NotAbsolute)
    );
    assert_eq!(
        Destination::external("javascript:alert(1)"),
        Err(UriError::DeniedScheme("javascript".into()))
    );
}

#[test]
fn a_current_link_is_the_current_one_of_what_its_destination_names() {
    assert_eq!(
        Destination::from(Location::root().segment("a")).current_kind(),
        CurrentKind::Page
    );
    assert_eq!(
        Destination::anchor("contact").current_kind(),
        CurrentKind::Location
    );
    assert_eq!(Destination::locale("en").current_kind(), CurrentKind::Item);
    assert_eq!(
        Destination::external("https://example.com")
            .unwrap()
            .current_kind(),
        CurrentKind::Item
    );
}
