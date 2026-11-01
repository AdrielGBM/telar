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
fn external_refuses_what_the_policy_does_not_allow_without_a_panic() {
    assert_eq!(external("no-scheme"), Err(UriError::NotAbsolute));
    assert_eq!(
        external("javascript:alert(1)"),
        Err(UriError::DeniedScheme("javascript".into()))
    );
}

#[test]
fn an_option_of_a_destination_flattens() {
    assert_eq!(
        Some(anchor("contact")).into_destination(),
        Some(anchor("contact"))
    );
    assert_eq!(None::<Destination>.into_destination(), None);
    assert_eq!(external("no-scheme").into_destination(), None);
    assert_eq!(
        external("https://example.com").into_destination(),
        external("https://example.com").ok()
    );
}

#[derive(Clone, Default)]
struct Captured(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Captured {
    type Writer = Self;

    fn make_writer(&'a self) -> Self {
        self.clone()
    }
}

#[test]
fn a_refused_external_uri_is_logged_without_raw_control_characters() {
    let captured = Captured::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(captured.clone())
        .with_ansi(false)
        .finish();
    let destination = tracing::subscriber::with_default(subscriber, || {
        // A test on another thread can register this callsite while no subscriber wants it and cache that answer; rebuilding counts this thread's subscriber in.
        tracing::callsite::rebuild_interest_cache();
        external("https://example.com/\u{1b}[31m").into_destination()
    });
    assert_eq!(destination, None);
    let log = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
    assert!(log.contains("refused"), "{log}");
    assert!(!log.contains('\u{1b}'), "{log:?}");
}
