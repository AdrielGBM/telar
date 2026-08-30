//! The catalogue, from a keyboard and from a screen reader.
//!
//! Every one of these widgets shipped answering the mouse and nothing else. There was no failing test to
//! notice it, because each component's own tests tapped it with a synthetic pointer — which is exactly what a
//! test written from the inside will do. This one asks the question from outside: can you get there, can you
//! work it, and can something be told what it is.

use std::cell::Cell;
use std::rc::Rc;

use layout_core::{AvailableSpace, LayoutStyle};
use platform_core::{Event, Key, NamedKey};
use reactive_core::signal;
use telar_ui_components::*;
use ui_core::focus::{self, Role};
use ui_core::{ComponentList, Container, LayoutItem, box_item, compute_layout};

/// Building a control measures its label, so this runs before the first widget exists.
fn install_metrics() {
    renderer_core::set_default_text_metrics(renderer_text::ShaperMetrics);
}

/// Lays `items` out inside a window-sized root, the way a surface would, and hands back a dispatchable tree.
///
/// The root *owns* them: a dropped widget unregisters its focus id, so a helper that laid out the nodes and
/// then let the widgets go would report an empty tab order and look like the bug it was meant to catch.
fn mount(items: Vec<Box<dyn LayoutItem>>) -> ComponentList {
    let root = Container::new(
        LayoutStyle::new().flex_column().width(400.0).height(400.0),
        items,
    )
    .unwrap();
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let list = ComponentList::new(box_item(root));
    let _ = list.commands();
    list
}

fn key(named: NamedKey) -> Event {
    Event::KeyPressed {
        key: Key::Named(named),
        modifiers: platform_core::ModifiersState::default(),
    }
}

/// The plain case, and the one that was false for every button in the catalogue: Tab arrives, Enter fires.
#[test]
fn a_button_can_be_reached_and_pressed_without_a_mouse() {
    install_metrics();
    ui_core::reset_layout_runtime();
    focus::clear();
    let fired: Rc<Cell<bool>> = Rc::new(Cell::new(false));
    let sink = fired.clone();
    let item = button(
        ButtonProps::props()
            .label("Save")
            .on_press(Box::new(move || sink.set(true)))
            .build(),
    )
    .unwrap();
    let mut tree = mount(vec![item]);

    focus::focus_next();
    assert!(focus::current().is_some(), "Tab reaches the button");
    tree.on_event(&key(NamedKey::Enter));
    assert!(fired.get(), "and Enter presses it");
}

/// A slider is reachable *and* adjustable: arrows are the keyboard's only way to move a continuous value, and
/// without them being a tab stop just means the keyboard can get stuck on it.
#[test]
fn a_slider_moves_under_the_arrow_keys() {
    install_metrics();
    ui_core::reset_layout_runtime();
    focus::clear();
    let value = signal(0.5f32);
    let item = slider(SliderProps::props().value(value).step(0.25).build()).unwrap();
    let mut tree = mount(vec![item]);

    focus::focus_next();
    tree.on_event(&key(NamedKey::ArrowRight));
    assert_eq!(value.get(), 0.75);
    tree.on_event(&key(NamedKey::ArrowLeft));
    tree.on_event(&key(NamedKey::ArrowLeft));
    assert_eq!(value.get(), 0.25);
}

/// Enter on a focused checkbox toggles it, which is the same commit a tap makes — one path, not two that
/// drift.
#[test]
fn enter_toggles_a_focused_checkbox() {
    install_metrics();
    ui_core::reset_layout_runtime();
    focus::clear();
    let checked = signal(false);
    let item = checkbox(
        CheckboxProps::props()
            .checked(checked)
            .label("Wireframe")
            .build(),
    )
    .unwrap();
    let mut tree = mount(vec![item]);

    focus::focus_next();
    tree.on_event(&key(NamedKey::Enter));
    assert!(checked.get(), "Enter commits what a tap would");
}

/// A control with a state announces the state it is actually in. Saying "checkbox" and stopping there leaves
/// the user to guess; defaulting to unticked is worse, because it is confidently wrong for half of them.
#[test]
fn a_checked_box_is_announced_as_checked() {
    install_metrics();
    ui_core::reset_layout_runtime();
    focus::clear();
    let checked = signal(true);
    let item = checkbox(
        CheckboxProps::props()
            .checked(checked)
            .label("Wireframe")
            .build(),
    )
    .unwrap();
    let _tree = mount(vec![item]);

    assert_eq!(focus::exposed()[0].toggled, Some(true));
    checked.set(false);
    assert_eq!(
        focus::exposed()[0].toggled,
        Some(false),
        "and follows it without the widget being rebuilt"
    );
}

/// And what a reader is told. The role is the part no amount of pointer testing would have caught: a checkbox
/// that announced itself as a button would be operable and still wrong.
#[test]
fn the_catalogue_says_what_each_control_is() {
    install_metrics();
    ui_core::reset_layout_runtime();
    focus::clear();
    let items: Vec<Box<dyn LayoutItem>> = vec![
        button(ButtonProps::props().label("Save").build()).unwrap(),
        checkbox(CheckboxProps::props().label("Wireframe").build()).unwrap(),
        toggle(ToggleProps::props().label("Snap").build()).unwrap(),
        slider(SliderProps::props().build()).unwrap(),
    ];
    let _tree = mount(items);

    let roles: Vec<Role> = focus::exposed().into_iter().map(|e| e.role).collect();
    assert_eq!(
        roles,
        vec![Role::Button, Role::CheckBox, Role::Switch, Role::Slider],
        "each control reports what it is, in the order it was built"
    );
}
