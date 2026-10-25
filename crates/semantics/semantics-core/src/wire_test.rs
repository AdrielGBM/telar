use std::collections::HashSet;
use std::fmt::Debug;
use std::sync::Arc;

use wire_core::{Wire, decode, encode};

use super::*;

fn round_trip<T: Wire + PartialEq + Debug>(value: T) {
    assert_eq!(decode::<T>(&encode(&value)), Ok(value));
}

fn link_round_trip(link: Option<Destination>) -> Option<Destination> {
    let mut w = Writer::new();
    put_link(&mut w, link.as_ref());
    let mut r = Reader::new(w.as_slice());
    let read = take_link(&mut r).unwrap();
    assert!(r.is_empty());
    read
}

#[test]
fn every_role_tag_reads_back_as_a_role_of_its_own() {
    let mut roles = HashSet::new();
    for tag in 0..=47u8 {
        let bytes = if tag == 10 { vec![tag, 2] } else { vec![tag] };
        let role = decode::<Role>(&bytes).unwrap();
        round_trip(role);
        roles.insert(role);
    }
    assert_eq!(roles.len(), 48);
    assert_eq!(
        decode::<Role>(&[48]),
        Err(WireError::Tag {
            of: "Role",
            tag: 48
        })
    );
}

#[test]
fn every_reading_value_round_trips() {
    round_trip(Orientation::Horizontal);
    round_trip(Orientation::Vertical);
    round_trip(SortDirection::Ascending);
    round_trip(SortDirection::Descending);
    round_trip(HasPopup::Menu);
    round_trip(HasPopup::ListBox);
    round_trip(CurrentKind::Page);
    round_trip(CurrentKind::Location);
    round_trip(CurrentKind::Item);
    round_trip(NumericValue {
        now: 0.25,
        min: -1.0,
        max: 1.0,
    });
    round_trip(SetPosition {
        level: 2,
        position: 3,
        size: 40,
    });
    round_trip(Modal {
        layer: 7,
        opened_from: Some(9),
    });
    round_trip(Focusable {
        tab_stop: true,
        consumes: ConsumedKeys::TAB.with(ConsumedKeys::ENTER),
    });
}

#[test]
fn a_location_round_trips_with_everything_it_can_name() {
    round_trip(Location::root());
    round_trip(
        Location::from_segments(["docs", "ñandú"])
            .with_locale("es-CL")
            .with_fragment("intro")
            .with_param("tag", "a")
            .with_param("tag", "b"),
    );
}

#[test]
fn every_kind_of_link_round_trips() {
    for link in [
        None,
        Some(Destination::Route(Location::from_segments(["a", "b"]))),
        Some(Destination::anchor("settings")),
        Some(Destination::locale("de")),
        Some(Destination::external("https://example.com/a?b=c").unwrap()),
    ] {
        assert_eq!(link_round_trip(link.clone()), link);
    }
}

fn external_link_bytes(address: &str) -> Vec<u8> {
    let mut w = Writer::new();
    w.u8(1);
    w.u8(EXTERNAL);
    w.str(address);
    w.into_vec()
}

#[test]
fn a_link_to_a_scheme_this_side_refuses_arrives_as_no_link() {
    for address in ["unregistered-scheme:x", "javascript:alert(1)"] {
        let bytes = external_link_bytes(address);
        let mut r = Reader::new(&bytes);
        assert_eq!(take_link(&mut r), Ok(None));
        assert!(r.is_empty());
    }
}

#[test]
fn a_link_that_is_no_uri_at_all_is_malformed() {
    let bytes = external_link_bytes("no scheme here");
    assert_eq!(
        take_link(&mut Reader::new(&bytes)),
        Err(WireError::Invalid("a link that is no URI"))
    );
}

fn every_field_set() -> Semantics {
    Semantics {
        role: Role::Heading(3),
        label: Some(Arc::from("Title")),
        link: Some(Destination::anchor("top")),
        focused: true,
        toggled: Some(false),
        disabled: true,
        click_through: true,
        focusable: Some(Focusable {
            tab_stop: false,
            consumes: ConsumedKeys::SPACE,
        }),
        lang: Some(Arc::from("en")),
        hidden: true,
        anchor: Some(Arc::from("here")),
        current: true,
        value: Some(NumericValue {
            now: 1.0,
            min: 0.0,
            max: 2.0,
        }),
        orientation: Some(Orientation::Vertical),
        expanded: Some(true),
        position: Some(SetPosition {
            level: 1,
            position: 2,
            size: 3,
        }),
        active_descendant: Some(4),
        controls: Some(5),
        has_popup: Some(HasPopup::ListBox),
        keyboard_shortcut: Some(Arc::from("Control+S")),
        sort: Some(SortDirection::Descending),
        multiselectable: true,
        modal: Some(Modal {
            layer: 1,
            opened_from: None,
        }),
    }
}

#[test]
fn semantics_round_trip_with_every_field_set_and_with_none() {
    round_trip(every_field_set());
    round_trip(Semantics::default());
}

#[test]
fn a_box_whose_link_is_refused_still_arrives_whole() {
    let semantics = Semantics {
        link: Some(Destination::external("https://example.com").unwrap()),
        ..every_field_set()
    };
    let mut bytes = encode(&semantics);
    let at = bytes
        .windows(5)
        .position(|window| window == b"https")
        .unwrap();
    bytes[at..at + 5].copy_from_slice(b"zzzzz");
    assert_eq!(
        decode::<Semantics>(&bytes),
        Ok(Semantics {
            link: None,
            ..semantics
        })
    );
}

#[test]
fn the_bytes_of_every_semantic_value_are_pinned_to_the_wire_version() {
    let roles = [
        Role::Group,
        Role::Banner,
        Role::Navigation,
        Role::Main,
        Role::Complementary,
        Role::ContentInfo,
        Role::Article,
        Role::Section,
        Role::Form,
        Role::Search,
        Role::Heading(1),
        Role::List,
        Role::ListItem,
        Role::ScrollArea,
        Role::Drawing,
        Role::Dialog,
        Role::Button,
        Role::Link,
        Role::CheckBox,
        Role::Radio,
        Role::Switch,
        Role::TabList,
        Role::Tab,
        Role::TabPanel,
        Role::Menu,
        Role::MenuItem,
        Role::MenuItemCheckBox,
        Role::MenuItemRadio,
        Role::ListBox,
        Role::Option,
        Role::Slider,
        Role::SpinButton,
        Role::TextInput,
        Role::MultilineTextInput,
        Role::ComboBox,
        Role::Disclosure,
        Role::ProgressBar,
        Role::Tree,
        Role::Grid,
        Role::Row,
        Role::GridCell,
        Role::ColumnHeader,
        Role::TreeItem,
        Role::Toolbar,
        Role::Splitter,
        Role::Status,
        Role::Log,
        Role::Label,
    ];
    let mut bytes: Vec<u8> = roles.iter().flat_map(encode).collect();
    bytes.extend(encode(&Role::Heading(6)));
    for value in [Orientation::Horizontal, Orientation::Vertical] {
        bytes.extend(encode(&value));
    }
    for value in [SortDirection::Ascending, SortDirection::Descending] {
        bytes.extend(encode(&value));
    }
    for value in [HasPopup::Menu, HasPopup::ListBox] {
        bytes.extend(encode(&value));
    }
    for value in [CurrentKind::Page, CurrentKind::Location, CurrentKind::Item] {
        bytes.extend(encode(&value));
    }
    bytes.extend(encode(&NumericValue {
        now: 0.25,
        min: -1.0,
        max: 1.0,
    }));
    bytes.extend(encode(&SetPosition {
        level: 2,
        position: 3,
        size: 40,
    }));
    bytes.extend(encode(&Modal {
        layer: 7,
        opened_from: Some(9),
    }));
    for keys in [
        ConsumedKeys::TAB,
        ConsumedKeys::SPACE,
        ConsumedKeys::ENTER,
        ConsumedKeys::BACKSPACE,
        ConsumedKeys::ARROW_UP,
        ConsumedKeys::ARROW_DOWN,
        ConsumedKeys::ARROW_LEFT,
        ConsumedKeys::ARROW_RIGHT,
        ConsumedKeys::PAGE_UP,
        ConsumedKeys::PAGE_DOWN,
        ConsumedKeys::HOME,
        ConsumedKeys::END,
    ] {
        bytes.extend(encode(&keys));
    }
    bytes.extend(encode(&Focusable {
        tab_stop: true,
        consumes: ConsumedKeys::TAB.with(ConsumedKeys::ENTER),
    }));
    bytes.extend(encode(
        &Location::from_segments(["docs", "ñandú"])
            .with_locale("es-CL")
            .with_fragment("intro")
            .with_param("tag", "a")
            .with_param("tag", "b"),
    ));
    bytes.extend(encode(&Location::root()));
    bytes.extend(encode(&every_field_set()));
    bytes.extend(encode(&Semantics::default()));
    wire_core::assert_golden!("src/wire_golden/semantics.hex", &bytes);
}

#[test]
fn the_bytes_of_every_kind_of_link_are_pinned_to_the_wire_version() {
    let mut w = Writer::new();
    for link in [
        None,
        Some(Destination::Route(Location::from_segments(["a", "b"]))),
        Some(Destination::anchor("settings")),
        Some(Destination::locale("de")),
        Some(Destination::external("https://example.com/a?b=c").unwrap()),
    ] {
        put_link(&mut w, link.as_ref());
    }
    wire_core::assert_golden!("src/wire_golden/links.hex", w.as_slice());
}
