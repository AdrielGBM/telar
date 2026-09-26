use std::sync::Arc;

use layout_core::NodeId;
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

fn open(node: NodeId, bounds: Rect) -> DrawCommand {
    DrawCommand::PushElement {
        element: Arc::new(renderer_core::Element::new(
            renderer_core::ElementId(node.into()),
            renderer_core::Semantics::group(),
            "",
            bounds,
        )),
    }
}

fn letter(c: &'static str) -> Text {
    Text::new(
        move || c.to_string(),
        LayoutStyle::new().width(10.0).height(16.0),
        || TextStyle::new(12.0, renderer_core::Color::BLACK),
    )
    .unwrap()
}

/// Split letters: the row is read as the word it spells, once, and none of the letters is read on its own.
#[test]
fn a_named_box_over_hidden_letters_reads_as_one_word() {
    use crate::annotation::Accessible;
    reset_layout_runtime();
    focus::clear();
    let h = letter("H").a11y_hidden();
    let i = letter("i").a11y_hidden();
    let (h_node, i_node) = (h.layout_node(), i.layout_node());
    let word = Container::new(
        LayoutStyle::new().flex_row(),
        vec![box_item(h), box_item(i)],
    )
    .unwrap()
    .a11y_label(|| "Hi")
    .a11y_lang(|| "en");
    compute_layout(
        word.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();

    let frame = [
        open(word.layout_node(), rect(0.0, 0.0, 20.0, 16.0)),
        open(h_node, rect(0.0, 0.0, 10.0, 16.0)),
        text_at("H", rect(0.0, 0.0, 10.0, 16.0)),
        DrawCommand::PopElement,
        open(i_node, rect(10.0, 0.0, 10.0, 16.0)),
        text_at("i", rect(10.0, 0.0, 10.0, 16.0)),
        DrawCommand::PopElement,
        DrawCommand::PopElement,
    ];
    let nodes = snapshot(&frame);
    assert_eq!(nodes.len(), 1, "one word, not a word and its letters");
    assert_eq!(nodes[0].name, "Hi");
    assert_eq!(nodes[0].role, Role::Label);
    assert_eq!(nodes[0].lang.as_deref(), Some("en"));
    assert_eq!(platform_core::accessibility::transcript(&nodes), "Hi");
}

/// A language reaches everything under the box that said it, and a nearer one wins.
#[test]
fn a_language_reaches_the_text_beneath_it() {
    use crate::annotation::Accessible;
    reset_layout_runtime();
    focus::clear();
    let quote = letter("Hola").a11y_lang(|| "es");
    let quote_node = quote.layout_node();
    let page = Container::new(LayoutStyle::new(), vec![box_item(quote)])
        .unwrap()
        .a11y_lang(|| "en");
    let frame = [
        open(page.layout_node(), rect(0.0, 0.0, 100.0, 40.0)),
        text_at("Hello", rect(0.0, 0.0, 40.0, 16.0)),
        open(quote_node, rect(0.0, 20.0, 40.0, 16.0)),
        text_at("Hola", rect(0.0, 20.0, 40.0, 16.0)),
        DrawCommand::PopElement,
        DrawCommand::PopElement,
    ];
    let nodes = snapshot(&frame);
    let lang_of = |name: &str| {
        nodes
            .iter()
            .find(|n| n.name == name)
            .and_then(|n| n.lang.clone())
    };
    assert_eq!(lang_of("Hello").as_deref(), Some("en"));
    assert_eq!(lang_of("Hola").as_deref(), Some("es"));
}

/// A control the application named is announced by that name, and not by the glyph it draws.
#[test]
fn a_named_control_is_not_renamed_by_its_icon() {
    use crate::annotation::Accessible;
    reset_layout_runtime();
    focus::clear();
    let close = StyledContainer::new(
        LayoutStyle::new().width(30.0).height(30.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .control(Role::Button)
    .on_press(|| {})
    .a11y_label(|| "Close");
    let node = close.layout_node();
    let root = Container::new(LayoutStyle::new().flex_column(), vec![box_item(close)]).unwrap();
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();

    let nodes = snapshot(&[
        open(node, rect(0.0, 0.0, 30.0, 30.0)),
        text_at("×", rect(5.0, 5.0, 20.0, 20.0)),
        DrawCommand::PopElement,
    ]);
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].name, "Close");
    assert_eq!(nodes[0].role, Role::Button);
    assert_eq!(
        platform_core::accessibility::transcript(&nodes),
        "Close, button"
    );
}

/// A control inside a hidden box is not offered to a reader either: hiding takes the subtree.
#[test]
fn a_hidden_box_takes_its_controls_with_it() {
    use crate::annotation::Accessible;
    reset_layout_runtime();
    focus::clear();
    let button = StyledContainer::new(
        LayoutStyle::new().width(30.0).height(30.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .control(Role::Button)
    .on_press(|| {});
    let button_node = button.layout_node();
    let decoration = Container::new(LayoutStyle::new(), vec![box_item(button)])
        .unwrap()
        .a11y_hidden();
    compute_layout(
        decoration.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();

    let nodes = snapshot(&[
        open(decoration.layout_node(), rect(0.0, 0.0, 30.0, 30.0)),
        open(button_node, rect(0.0, 0.0, 30.0, 30.0)),
        text_at("Go", rect(0.0, 0.0, 30.0, 30.0)),
        DrawCommand::PopElement,
        DrawCommand::PopElement,
    ]);
    assert!(
        nodes.is_empty(),
        "nothing under a hidden box is read: {nodes:?}"
    );
}

/// A picture is read only when it is named, and then as an image.
#[test]
fn a_named_picture_is_read_as_an_image() {
    use crate::annotation::Accessible;
    reset_layout_runtime();
    focus::clear();
    let logo = letter("").a11y_label(|| "Company logo");
    let node = logo.layout_node();
    compute_layout(
        node,
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();
    let picture = DrawCommand::Image {
        data: Arc::new(renderer_core::ImageData::new(vec![0; 4], 1, 1)),
        rect: rect(0.0, 0.0, 10.0, 16.0),
        raster: renderer_core::Raster::Smooth,
        fill: renderer_core::ImageFill::Stretch,
    };
    let nodes = snapshot(&[
        open(node, rect(0.0, 0.0, 10.0, 16.0)),
        picture,
        DrawCommand::PopElement,
    ]);
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].role, Role::Drawing);
    assert_eq!(
        platform_core::accessibility::transcript(&nodes),
        "Company logo, image"
    );
}
