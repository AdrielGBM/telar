use layout_core::{AvailableSpace, LayoutStyle};
use platform_core::{Event, Ime};
use reactive_core::signal;
use renderer_core::{Color, TextStyle};
use ui_tree::Component;

use super::*;
use crate::container::Container;
use crate::context::{compute_layout, reset_layout_runtime};
use crate::input::Input;
use crate::layout_item::LayoutItem;
use crate::scroll_area::LayoutScrollArea;
use crate::{TextArea, focus};

fn style() -> TextStyle {
    TextStyle::new(14.0, Color::BLACK)
}

fn field(initial: &str) -> Input {
    Input::new(
        signal(initial.to_string()),
        LayoutStyle::new().width(200.0).height(20.0),
        style,
    )
    .unwrap()
}

fn spacer(height: f32) -> Box<dyn LayoutItem> {
    Box::new(Container::new(LayoutStyle::new().width(200.0).height(height), vec![]).unwrap())
}

fn lay_out(root: &impl LayoutItem) {
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
}

#[test]
fn no_field_holding_the_keyboard_reports_nothing() {
    reset_layout_runtime();
    let input = field("hello");
    let root = Container::new(LayoutStyle::new().width(400.0), vec![Box::new(input)]).unwrap();
    lay_out(&root);
    root.view();
    assert_eq!(text_input(), None);
}

#[test]
fn the_focused_field_reports_its_caret_where_the_surface_draws_it() {
    reset_layout_runtime();
    let input = field("hello");
    let id = input.focus_id();
    let root = Container::new(
        LayoutStyle::new().flex_column().width(400.0),
        vec![spacer(30.0), Box::new(input)],
    )
    .unwrap();
    lay_out(&root);
    focus::request(id);
    root.view();
    let reported = text_input().expect("a field holds the keyboard");
    assert_eq!(reported.caret.y, 30.0);
    assert!(reported.caret.x > 0.0, "after the word: {reported:?}");
    assert!(!reported.secret);

    focus::clear();
    root.view();
    assert_eq!(
        text_input(),
        None,
        "a field that let the keyboard go says nothing"
    );
}

#[test]
fn a_masked_field_asks_for_a_secret_entry() {
    reset_layout_runtime();
    let input = field("pin").secret();
    let id = input.focus_id();
    let root = Container::new(LayoutStyle::new().width(400.0), vec![Box::new(input)]).unwrap();
    lay_out(&root);
    focus::request(id);
    root.view();
    assert!(text_input().expect("focused").secret);
}

#[test]
fn a_scroll_moves_the_reported_caret_without_the_field_drawing_again() {
    reset_layout_runtime();
    let offset = (signal(0.0f32), signal(0.0f32));
    let input = field("");
    let id = input.focus_id();
    let rows: Vec<Box<dyn LayoutItem>> = vec![spacer(150.0), Box::new(input)];
    let scroll = LayoutScrollArea::new_keeping(
        LayoutStyle::new().width(300.0).height(200.0),
        offset,
        move |_| {
            Ok(
                Box::new(Container::new(LayoutStyle::new().flex_column(), rows)?)
                    as Box<dyn LayoutItem>,
            )
        },
    )
    .unwrap();
    lay_out(&scroll);
    focus::request(id);
    scroll.view();
    assert_eq!(text_input().expect("focused").caret.y, 150.0);
    offset.1.set(100.0);
    assert_eq!(text_input().expect("focused").caret.y, 50.0);
}

#[test]
fn a_wrapped_area_reports_the_row_its_caret_is_on() {
    reset_layout_runtime();
    let area = TextArea::new(
        signal("one two three four five six seven".to_string()),
        LayoutStyle::new().width(60.0),
        style,
    )
    .unwrap()
    .text_wrap(renderer_core::TextWrap::Wrap);
    let id = area.focus_id();
    let root = Container::new(LayoutStyle::new().width(60.0), vec![Box::new(area)]).unwrap();
    lay_out(&root);
    focus::request(id);
    root.view();
    let line = crate::text_metrics::line_box(&style());
    let caret = text_input().expect("focused").caret;
    assert!(
        caret.y >= line,
        "the end of the text is below the first row: {caret:?}"
    );
    assert_eq!(caret.height, line);
}

#[test]
fn the_reported_caret_follows_the_composition() {
    reset_layout_runtime();
    let mut input = field("ab");
    let id = input.focus_id();
    lay_out(&input);
    focus::request(id);
    input.view();
    let before = text_input().expect("focused").caret.x;
    input.on_event(&Event::Ime(Ime::preedit("かな", Some((6, 6)))));
    input.view();
    let after = text_input().expect("focused").caret.x;
    assert!(after > before, "{after} after {before}");
}
