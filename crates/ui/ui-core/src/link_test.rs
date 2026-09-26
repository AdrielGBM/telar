use std::cell::Cell;
use std::rc::Rc;
use std::sync::{Arc, Mutex, OnceLock};

use layout_core::{AvailableSpace, LayoutStyle};
use platform_core::{
    Event, IntoDestination, Key, Location, ModifiersState, NamedKey, PointerButton, PointerSource,
    anchor, external, location_history, receive_location_history,
};
use renderer_core::{RectStyle, Role};
use services_core::UriOpener;
use ui_tree::{Component, EventResult, RenderNode};

use super::*;
use crate::context::{compute_layout, reset_layout_runtime};
use crate::{LayoutItem, StyledContainer, focus};

#[derive(Default)]
struct Opened {
    uris: Mutex<Vec<String>>,
    beside: Mutex<Vec<String>>,
}

impl UriOpener for Opened {
    fn open(&self, uri: &str) -> bool {
        self.uris.lock().unwrap().push(uri.to_string());
        true
    }

    fn open_beside(&self, reference: &str) -> bool {
        self.beside.lock().unwrap().push(reference.to_string());
        true
    }
}

fn opened() -> &'static Arc<Opened> {
    static OPENED: OnceLock<Arc<Opened>> = OnceLock::new();
    OPENED.get_or_init(|| {
        let opened = Arc::new(Opened::default());
        services_core::set_uri_opener(opened.clone());
        opened
    })
}

fn link_to(destination: Destination) -> StyledContainer {
    link_reading(move || destination.clone())
}

fn link_reading<D: IntoDestination + 'static>(read: impl Fn() -> D + 'static) -> StyledContainer {
    reset_layout_runtime();
    focus::clear();
    receive_location_history(vec![Location::root()]);
    let link = StyledContainer::new(
        LayoutStyle::new().width(100.0).height(40.0),
        |_| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .to(read);
    compute_layout(
        link.layout_node(),
        AvailableSpace::Definite(100.0),
        AvailableSpace::Definite(40.0),
    )
    .unwrap();
    link
}

fn tap(link: &mut StyledContainer) {
    for event in [
        Event::PointerPressed {
            x: 10.0,
            y: 10.0,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        },
        Event::PointerReleased {
            x: 10.0,
            y: 10.0,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        },
    ] {
        link.on_event(&event);
    }
}

fn key(named: NamedKey) -> Event {
    Event::KeyPressed {
        key: Key::Named(named),
        modifiers: ModifiersState::default(),
    }
}

fn projects() -> Location {
    Location::root().segment("projects")
}

fn element_of(link: &StyledContainer, capture: bool) -> Arc<renderer_core::Element> {
    let was = ui_tree::set_element_capture(capture);
    let view = link.view();
    ui_tree::set_element_capture(was);
    match view {
        RenderNode::Element { element, .. } => element,
        _ => panic!("expected the box's element"),
    }
}

#[test]
fn a_tap_on_a_route_link_pushes_the_route() {
    let mut link = link_to(Destination::Route(projects()));
    tap(&mut link);
    assert_eq!(location_history(), vec![Location::root(), projects()]);
}

#[test]
fn enter_follows_a_link_and_space_does_not() {
    let mut link = link_to(Destination::Route(projects()));
    focus::focus_next();
    assert_eq!(link.on_event(&key(NamedKey::Space)), EventResult::Ignored);
    assert_eq!(location_history(), vec![Location::root()]);
    assert_eq!(link.on_event(&key(NamedKey::Enter)), EventResult::Handled);
    assert_eq!(location_history(), vec![Location::root(), projects()]);
}

#[test]
fn an_external_link_is_handed_to_the_system() {
    let opened = opened();
    let mut link = link_to(external("https://example.com/tapped").unwrap());
    tap(&mut link);
    assert!(
        opened
            .uris
            .lock()
            .unwrap()
            .contains(&"https://example.com/tapped".to_string())
    );
    assert_eq!(location_history(), vec![Location::root()]);
}

#[test]
fn an_invalid_runtime_external_renders_a_disabled_link_that_goes_nowhere() {
    let opened = opened();
    receive_location_history(vec![Location::root()]);
    let mut link = link_reading(|| external("not-a-uri"));
    focus::focus_next();
    tap(&mut link);
    assert_eq!(link.on_event(&key(NamedKey::Enter)), EventResult::Ignored);
    assert!(
        !opened
            .uris
            .lock()
            .unwrap()
            .iter()
            .any(|uri| uri.contains("not-a-uri"))
    );
    assert_eq!(location_history(), vec![Location::root()]);

    let element = element_of(&link, true);
    assert_eq!(element.semantics.role, Role::Link);
    assert!(element.semantics.disabled);
    assert_eq!(element.semantics.link, None);
}

#[test]
fn a_modified_press_opens_a_route_beside_where_the_target_can() {
    let opened = opened();
    let destination = Destination::Route(Location::root().segment("beside"));
    let ctrl = ModifiersState {
        is_ctrl: true,
        ..ModifiersState::default()
    };
    receive_location_history(vec![Location::root()]);
    assert!(follow_pressed(&destination, ctrl));
    assert!(
        opened
            .beside
            .lock()
            .unwrap()
            .contains(&"/beside".to_string())
    );
    assert_eq!(location_history(), vec![Location::root()]);
}

#[test]
fn an_anchor_is_revealed_only_while_something_answers_to_it() {
    let revealed = Rc::new(Cell::new(0));
    assert!(!reveal_anchor("contact"), "no box answers to it yet");
    let sink = revealed.clone();
    let registration = register_anchor("contact", move || {
        sink.set(sink.get() + 1);
        true
    });
    let mut link = link_to(anchor("contact"));
    tap(&mut link);
    assert_eq!(revealed.get(), 1);
    drop(registration);
    assert!(!follow(&anchor("contact")));
    assert_eq!(revealed.get(), 1);
}

#[test]
fn a_link_says_where_it_goes_and_keeps_no_keys() {
    let link = link_to(Destination::Route(projects()));
    let element = element_of(&link, true);
    assert_eq!(element.semantics.role, Role::Link);
    assert_eq!(element.semantics.link, Some(Destination::Route(projects())));
    assert_eq!(
        element.semantics.focusable.map(|f| f.consumes),
        Some(platform_core::ConsumedKeys::EMPTY)
    );
    let terminal = element_of(&link, false);
    assert_eq!(
        terminal.semantics.link,
        Some(Destination::Route(projects())),
        "a surface that reads no document still sees where a link goes"
    );
}

#[test]
fn a_document_follows_its_own_links_and_reports_them_back() {
    let mut link = link_to(Destination::Route(projects()));
    let was = ui_tree::set_element_capture(true);
    tap(&mut link);
    assert_eq!(location_history(), vec![Location::root()]);
    assert!(activate_box(link.layout_node().into()));
    ui_tree::set_element_capture(was);
    assert_eq!(location_history(), vec![Location::root(), projects()]);
    drop(link);
}

#[test]
fn a_box_that_is_gone_has_no_link_to_activate() {
    let link = link_to(Destination::Route(projects()));
    let id: u64 = link.layout_node().into();
    drop(link);
    assert!(!activate_box(id));
}
