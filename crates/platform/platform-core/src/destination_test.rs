use super::*;
use crate::receive_location_history;

#[derive(Clone)]
enum Page {
    Project(String),
}

impl Route for Page {
    fn to_location(&self) -> Location {
        match self {
            Page::Project(slug) => Location::root().segment("projects").segment(slug.clone()),
        }
    }

    fn from_location(_: &Location) -> Option<Self> {
        None
    }
}

#[test]
fn a_typed_route_a_location_and_a_destination_all_name_one() {
    let expected = Destination::Route(Location::root().segment("projects").segment("telar"));
    assert_eq!(
        Page::Project("telar".into()).into_destination(),
        Some(expected.clone())
    );
    assert_eq!(
        Location::root()
            .segment("projects")
            .segment("telar")
            .into_destination(),
        Some(expected)
    );
    assert_eq!(
        anchor("contact").into_destination(),
        Some(Destination::anchor("contact"))
    );
}

#[test]
fn each_destination_is_written_as_the_address_a_target_carries() {
    receive_location_history(vec![Location::root().segment("about")]);
    assert_eq!(
        address_of(&Page::Project("telar".into()).into_destination().unwrap()),
        "/projects/telar"
    );
    assert_eq!(address_of(&anchor("contact")), "/about#contact");
    assert_eq!(
        address_of(&external("mailto:someone@example.com").unwrap()),
        "mailto:someone@example.com"
    );
}

#[test]
fn external_names_no_scheme_is_refused_without_a_panic() {
    assert_eq!(external("no-scheme"), None);
}

#[test]
fn an_option_of_a_destination_flattens() {
    assert_eq!(
        Some(anchor("contact")).into_destination(),
        Some(anchor("contact"))
    );
    assert_eq!(None::<Destination>.into_destination(), None);
    assert_eq!(external("no-scheme").into_destination(), None);
}
