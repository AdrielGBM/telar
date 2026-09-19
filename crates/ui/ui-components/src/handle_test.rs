use platform_core::{Key, NamedKey};
use reactive_core::{RwSignal, Transaction, signal};
use renderer_core::RectStyle;
use ui_core::{ComponentList, StyledContainer};

use super::*;
use crate::harness::{hold, key_with, lay_out, moved, named, press, release, route};
use crate::test_support::fresh_layout_runtime;

struct Canvas {
    tree: ComponentList,
    value: RwSignal<f32>,
    clamped: RwSignal<bool>,
}

/// A 200x200 canvas holding a handle that sits at `(value, 50)` and reads its value off the pointer's x, clamped to `0..=100`.
fn canvas(start: f32) -> Canvas {
    fresh_layout_runtime();
    ui_core::reset_keyboard();
    ui_core::focus::clear();
    let value = signal(start);
    let clamped = signal(false);
    let dot = handle(
        HandleProps::props()
            .value(value)
            .to_value(Rc::new(|x, _| x))
            .to_point(Rc::new(|value| (value, 50.0)))
            .min(0.0)
            .max(100.0)
            .clamped(clamped)
            .build(),
        Children::default(),
    )
    .unwrap();
    let area = StyledContainer::new(
        LayoutStyle::new().width(200.0).height(200.0),
        |_| RectStyle::default(),
        vec![dot],
    )
    .unwrap();
    let node = area.layout_node();
    let tree = ComponentList::new(box_item(area));
    lay_out(node, 200.0, 200.0);
    Canvas {
        tree,
        value,
        clamped,
    }
}

impl Canvas {
    fn event(&mut self, event: platform_core::Event) {
        route(&mut self.tree, &event);
    }
}

#[test]
fn a_handle_follows_the_pointer_from_where_it_was_grabbed() {
    let mut canvas = canvas(20.0);
    canvas.event(press(22.0, 51.0));
    assert_eq!(canvas.value.peek(), 20.0, "the press itself moved nothing");
    canvas.event(moved(62.0, 51.0));
    assert_eq!(canvas.value.peek(), 60.0);
    canvas.event(moved(42.0, 90.0));
    assert_eq!(canvas.value.peek(), 40.0);
    canvas.event(release(42.0, 90.0));
    assert_eq!(canvas.value.peek(), 40.0);
}

#[test]
fn a_handle_clamps_and_shows_it_while_the_request_is_out_of_range() {
    let mut canvas = canvas(20.0);
    canvas.event(press(20.0, 50.0));
    canvas.event(moved(500.0, 50.0));
    assert_eq!(canvas.value.peek(), 100.0);
    assert!(canvas.clamped.peek(), "pulled past the bound");

    canvas.event(moved(50.0, 50.0));
    assert_eq!(canvas.value.peek(), 50.0);
    assert!(!canvas.clamped.peek(), "back in range");

    canvas.event(moved(-300.0, 50.0));
    assert_eq!(canvas.value.peek(), 0.0);
    assert!(canvas.clamped.peek());
    canvas.event(release(-300.0, 50.0));
    assert!(!canvas.clamped.peek(), "the drag ended");
}

#[test]
fn escape_mid_drag_puts_the_handle_back() {
    let mut canvas = canvas(20.0);
    canvas.event(press(20.0, 50.0));
    canvas.event(moved(80.0, 50.0));
    canvas.event(named(NamedKey::Escape));
    assert_eq!(canvas.value.peek(), 20.0);
    canvas.event(release(80.0, 50.0));
    assert_eq!(canvas.value.peek(), 20.0);
}

#[test]
fn a_focused_handle_is_nudged_by_the_arrow_keys() {
    let mut canvas = canvas(20.0);
    canvas.event(press(20.0, 50.0));
    canvas.event(release(20.0, 50.0));

    canvas.event(named(NamedKey::ArrowRight));
    assert_eq!(canvas.value.peek(), 21.0);
    let shift = hold(true, false);
    canvas.event(key_with(Key::Named(NamedKey::ArrowLeft), shift));
    assert_eq!(canvas.value.peek(), 11.0);
    hold(false, false);
    canvas.value.set(99.5);
    canvas.event(named(NamedKey::ArrowUp));
    assert_eq!(canvas.value.peek(), 100.0);
    assert!(canvas.clamped.peek(), "a nudge past the bound shows it too");
}

#[test]
fn a_handle_joins_a_popover_transaction_on_the_same_value() {
    fresh_layout_runtime();
    let value = signal(10.0);
    let popover = Transaction::new(value);
    let dot = handle(
        HandleProps::props()
            .transaction(popover)
            .to_point(Rc::new(|value| (value, 50.0)))
            .build(),
        Children::default(),
    )
    .unwrap();
    let node = dot.layout_node();
    let mut tree = ComponentList::new(dot);
    lay_out(node, 200.0, 200.0);
    popover.begin().unwrap();
    route(&mut tree, &press(10.0, 50.0));
    route(&mut tree, &moved(70.0, 50.0));
    route(&mut tree, &release(70.0, 50.0));
    assert_eq!(value.peek(), 70.0);
    assert!(popover.is_open());
    popover.revert().unwrap();
    assert_eq!(value.peek(), 10.0);
}
