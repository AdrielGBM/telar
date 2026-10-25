use std::cell::Cell;
use std::rc::Rc;

use geometry_core::Rect;
use layout_core::{AvailableSpace, LayoutStyle};

use super::*;
use crate::container::Container;
use crate::context::{compute_layout, reset_layout_runtime};
use crate::layout_item::LayoutItem;

/// A drawn surface with no keyboard of its own, whose field's caret is what the closure says.
struct Caret(Box<dyn Fn() -> Option<(TextInput, Transform)>>);

impl Keyboard for Caret {
    fn text_entry_focused(&self) -> Option<bool> {
        None
    }

    fn keeps(&self, _keys: platform_core::ConsumedKeys) -> Option<bool> {
        None
    }
}

impl Inside for Caret {
    fn arrive(&self, _backwards: bool) {}

    fn tab(&self, _key: &Event) -> bool {
        false
    }

    fn blur(&self) {}

    fn holds_focus(&self) -> bool {
        false
    }

    fn text_input(&self) -> Option<(TextInput, Transform)> {
        (self.0)()
    }
}

fn input(field: u64, caret: Rect) -> TextInput {
    TextInput {
        field,
        caret,
        secret: true,
    }
}

/// A box at (20, 20), laid out and drawn, and its node.
fn drawn_box() -> (Container, NodeId) {
    let inner = Container::new(LayoutStyle::new().width(100.0).height(40.0), vec![]).unwrap();
    let node = inner.layout_node();
    let outer = Container::new(
        LayoutStyle::new()
            .width(400.0)
            .height(400.0)
            .padding_all(20.0),
        vec![Box::new(inner)],
    )
    .unwrap();
    compute_layout(
        outer.layout_node(),
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    ui_tree::Component::view(&outer);
    (outer, node)
}

#[test]
fn the_stop_reports_the_drawn_surfaces_caret_where_the_surface_around_shows_it() {
    reset_layout_runtime();
    focus::clear();
    let (_outer, node) = drawn_box();
    let stop = FrameFocus::register(
        node,
        Caret(Box::new(|| {
            Some((
                input(7, Rect::new(5.0, 4.0, 1.0, 10.0)),
                Transform::scale_around(2.0, 2.0, 0.0, 0.0),
            ))
        })),
    );
    focus::request(stop.id());

    assert!(stop.holds_keys());
    assert_eq!(
        crate::text_input(),
        Some(input(
            7u64.rotate_left(32) ^ stop.id(),
            Rect::new(30.0, 28.0, 2.0, 20.0)
        ))
    );
}

/// Two frames drawing surfaces whose fields have the same id there are told apart by their stops.
#[test]
fn the_same_field_inside_two_frames_is_two_fields_here() {
    reset_layout_runtime();
    focus::clear();
    let (_outer, node) = drawn_box();
    let caret = Rect::new(0.0, 0.0, 1.0, 10.0);
    let first = FrameFocus::register(
        node,
        Caret(Box::new(move || {
            Some((input(1, caret), Transform::IDENTITY))
        })),
    );
    let second = FrameFocus::register(
        node,
        Caret(Box::new(move || {
            Some((input(1, caret), Transform::IDENTITY))
        })),
    );
    focus::request(first.id());
    let one = crate::text_input().unwrap().field;
    focus::request(second.id());
    let other = crate::text_input().unwrap().field;
    assert_ne!(one, other);
}

#[test]
fn a_dropped_stop_leaves_the_tab_order_and_gives_up_focus() {
    reset_layout_runtime();
    focus::clear();
    let (_outer, node) = drawn_box();
    let asked = Rc::new(Cell::new(0));
    let counted = Rc::clone(&asked);
    let stop = FrameFocus::register(
        node,
        Caret(Box::new(move || {
            counted.set(counted.get() + 1);
            None
        })),
    );
    let id = stop.id();
    assert!(focus::exposed().iter().any(|exposed| exposed.id == id));
    focus::request(id);
    assert_eq!(crate::text_input(), None);
    assert_eq!(asked.get(), 1);

    drop(stop);
    assert_eq!(focus::current(), None);
    assert!(focus::exposed().iter().all(|exposed| exposed.id != id));
    focus::request(id);
    assert_eq!(crate::text_input(), None);
    assert_eq!(asked.get(), 1, "the caret is no longer asked for");
}

#[test]
fn keys_and_text_reach_the_drawn_surface_only_while_the_stop_holds_them() {
    use platform_core::{Ime, Key, ModifiersState, NamedKey};
    let key = |key| Event::KeyPressed {
        key,
        modifiers: ModifiersState::default(),
        unmodified: None,
    };
    let tab = key(Key::Named(NamedKey::Tab));
    let letter = key(Key::Char('a'));
    let text = Event::Ime(Ime::Commit("a".into()));

    assert_eq!(KeyRoute::of(&tab, true), Some(KeyRoute::Tab));
    assert_eq!(KeyRoute::of(&letter, true), Some(KeyRoute::Inside));
    assert_eq!(KeyRoute::of(&text, true), Some(KeyRoute::Inside));
    for event in [&tab, &letter, &text] {
        assert_eq!(KeyRoute::of(event, false), Some(KeyRoute::Outside));
    }
    assert_eq!(KeyRoute::of(&Event::CursorLeft, true), None);
}
