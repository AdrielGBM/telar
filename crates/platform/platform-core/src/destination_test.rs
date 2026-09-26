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
    assert_eq!(Page::Project("telar".into()).into_destination(), expected);
    assert_eq!(
        Location::root()
            .segment("projects")
            .segment("telar")
            .into_destination(),
        expected
    );
    assert_eq!(
        anchor("contact").into_destination(),
        Destination::anchor("contact")
    );
}

#[test]
fn each_destination_is_written_as_the_address_a_target_carries() {
    receive_location_history(vec![Location::root().segment("about")]);
    assert_eq!(
        address_of(&Page::Project("telar".into()).into_destination()),
        "/projects/telar"
    );
    assert_eq!(address_of(&anchor("contact")), "/about#contact");
    assert_eq!(
        address_of(&external("mailto:someone@example.com")),
        "mailto:someone@example.com"
    );
}
