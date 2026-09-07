use std::sync::Arc;

use layout_core::{AvailableSpace, LayoutStyle};
use renderer_core::{RectStyle, TextStyle};

use super::*;
use crate::container::Container;
use crate::context::{compute_layout, reset_layout_runtime};
use crate::layout_item::{LayoutItem, box_item};
use crate::styled_container::StyledContainer;
use crate::text::Text;

fn text_at(text: &str, rect: Rect) -> DrawCommand {
    DrawCommand::Text {
        spans: None,
        text: Arc::from(text),
        rect,
        style: Arc::new(TextStyle::new(12.0, renderer_core::Color::BLACK)),
    }
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::new(x, y, w, h)
}

/// A button built the ordinary way is announced with the label it draws — nobody wrote an accessible name for it, and that is the point: a second copy of the text is a second thing to keep true.
#[test]
fn a_control_is_named_by_the_text_it_draws() {
    reset_layout_runtime();
    focus::clear();
    let label = Text::new(
        || "Save".to_string(),
        LayoutStyle::new().width(40.0).height(16.0),
        || TextStyle::new(12.0, renderer_core::Color::BLACK),
    )
    .unwrap();
    let button = StyledContainer::new(
        LayoutStyle::new().width(80.0).height(30.0),
        |_r| RectStyle::default(),
        vec![box_item(label)],
    )
    .unwrap()
    .control(Role::Button)
    .on_press(|| {});
    let root = Container::new(LayoutStyle::new().flex_column(), vec![box_item(button)]).unwrap();
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();

    let nodes = snapshot(&[text_at("Save", rect(10.0, 5.0, 40.0, 16.0))]);
    let button = nodes
        .iter()
        .find(|n| n.id.is_some())
        .expect("the button is exposed");
    assert_eq!(button.name, "Save");
    assert_eq!(button.role, Role::Button);
    assert!(
        button.enabled,
        "a control is exposed enabled unless it says otherwise"
    );
}

/// Text belonging to no control is content, not noise. A reader handed only the buttons cannot say what the buttons are for.
#[test]
fn text_outside_any_control_is_still_announced() {
    reset_layout_runtime();
    focus::clear();
    let nodes = snapshot(&[text_at("Delete everything?", rect(0.0, 0.0, 200.0, 20.0))]);
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].role, Role::Label);
    assert_eq!(nodes[0].name, "Delete everything?");
    assert_eq!(nodes[0].id, None);
}

/// Nesting: the label goes to the smallest control that contains it, so a button inside a card is named by its own text rather than by everything the card happens to hold.
#[test]
fn a_label_belongs_to_the_smallest_control_around_it() {
    let outer = rect(0.0, 0.0, 200.0, 100.0);
    let inner = rect(10.0, 10.0, 50.0, 20.0);
    assert!(
        contains(outer, inner),
        "the outer box encloses the inner one"
    );
    assert!(
        contains(inner, rect(20.0, 15.0, 10.0, 8.0)),
        "and the inner one encloses the label"
    );
    assert!(
        !contains(inner, rect(120.0, 15.0, 10.0, 8.0)),
        "a label to the side of it is not inside"
    );
    assert!(
        area(inner) < area(outer),
        "the inner box is the smaller of the two, so the label is its"
    );
}

/// A value control that reports only its role has not said the one thing it exists to report: a slider announced as "Volume, slider" leaves the number — the whole content of the control — unsaid.
#[test]
fn a_valued_control_reports_where_it_stands() {
    reset_layout_runtime();
    focus::clear();
    let track = StyledContainer::new(
        LayoutStyle::new().width(200.0).height(20.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .control(Role::Slider)
    .valued(|| platform_core::NumericValue {
        now: 0.25,
        min: 0.0,
        max: 1.0,
    });
    let root = Container::new(LayoutStyle::new().flex_column(), vec![box_item(track)]).unwrap();
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();

    let nodes = snapshot(&[]);
    let slider = nodes.iter().find(|n| n.id.is_some()).expect("exposed");
    let value = slider.value.expect("a slider carries a number");
    assert_eq!(value.now, 0.25);
    assert_eq!((value.min, value.max), (0.0, 1.0));
}

/// `toggled` was populated only through `labelled_control`, so a tab never said it was the selected one.
#[test]
fn a_control_that_carries_a_state_reports_it() {
    reset_layout_runtime();
    focus::clear();
    let tab = StyledContainer::new(
        LayoutStyle::new().width(80.0).height(30.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .control(Role::Tab)
    .toggled(|| true);
    let root = Container::new(LayoutStyle::new().flex_column(), vec![box_item(tab)]).unwrap();
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();

    let nodes = snapshot(&[]);
    let tab = nodes.iter().find(|n| n.id.is_some()).expect("exposed");
    assert_eq!(tab.toggled, Some(true));
}

/// The reading order a reader walks is the order things sit on screen, not the order they were built — which is what tab order is, and stays.
#[test]
fn nodes_come_back_in_reading_order() {
    reset_layout_runtime();
    focus::clear();
    let nodes = snapshot(&[
        text_at("second", rect(0.0, 40.0, 50.0, 16.0)),
        text_at("first", rect(0.0, 10.0, 50.0, 16.0)),
        text_at("third", rect(60.0, 40.0, 50.0, 16.0)),
    ]);
    let names: Vec<&str> = nodes.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names, vec!["first", "second", "third"]);
}
