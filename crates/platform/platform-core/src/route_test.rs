use super::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TestRoute {
    Home,
    Act(u8),
    Credits,
}

impl super::Route for TestRoute {
    fn to_location(&self) -> Location {
        match self {
            TestRoute::Home => Location::root(),
            TestRoute::Act(n) => Location::root().segment("acts").segment(n.to_string()),
            TestRoute::Credits => Location::root().segment("credits"),
        }
    }

    fn from_location(location: &Location) -> Option<Self> {
        match location.segments() {
            [] => Some(TestRoute::Home),
            [acts, n] if acts == "acts" => n.parse().ok().map(TestRoute::Act),
            [credits] if credits == "credits" => Some(TestRoute::Credits),
            _ => None,
        }
    }

    fn title(&self) -> Option<String> {
        match self {
            TestRoute::Home => None,
            TestRoute::Act(n) => Some(format!("Act {n}")),
            TestRoute::Credits => Some("Credits".to_string()),
        }
    }
}

#[test]
fn round_trips_through_a_location() {
    for route in [TestRoute::Home, TestRoute::Act(2), TestRoute::Credits] {
        let location = route.to_location();
        assert_eq!(TestRoute::from_location(&location), Some(route));
    }
}

#[test]
fn fragment_and_params_survive_the_round_trip_untouched_by_the_route() {
    let location = TestRoute::Act(1)
        .to_location()
        .with_fragment("diagram")
        .with_param("lang", "es");
    assert_eq!(TestRoute::from_location(&location), Some(TestRoute::Act(1)));
    assert_eq!(location.fragment(), Some("diagram"));
    assert_eq!(location.param("lang"), Some("es"));
}

#[test]
fn unknown_location_is_not_a_route() {
    let location = Location::root().segment("nowhere");
    assert_eq!(TestRoute::from_location(&location), None);
}

#[test]
fn title_hook_is_optional() {
    assert_eq!(TestRoute::Home.title(), None);
    assert_eq!(TestRoute::Credits.title(), Some("Credits".to_string()));
}
