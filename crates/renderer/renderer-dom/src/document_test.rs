use std::sync::Arc;

use geometry_core::Rect;
use renderer_core::{Color, DrawCommand, Element, ElementId, Semantics, TextStyle};

use super::*;

struct Fixed;

impl Surface for Fixed {
    fn hold_document_scroll(&mut self, _id: u64) {}

    fn fixed_origin(&mut self) -> Option<(f32, f32)> {
        None
    }

    fn image_href(&mut self, _data: &ImageData) -> Option<Rc<str>> {
        None
    }

    fn baseline(&mut self, style: &TextStyle) -> f32 {
        style.font_size
    }
}

fn open(id: u64, semantics: Semantics) -> DrawCommand {
    DrawCommand::PushElement {
        element: Arc::new(Element::new(
            ElementId(id),
            semantics,
            "",
            Rect::new(0.0, 0.0, 50.0, 20.0),
        )),
    }
}

fn text(content: &str) -> DrawCommand {
    DrawCommand::Text {
        text: Arc::from(content),
        spans: None,
        rect: Rect::new(0.0, 0.0, 50.0, 20.0),
        style: Arc::new(TextStyle::new(14.0, Color::BLACK)),
    }
}

fn only_box(frame: &Frame) -> &BoxNode {
    match frame.children.as_slice() {
        [Node::Box(node)] => node,
        _ => panic!("one box at the top"),
    }
}

#[test]
fn a_single_run_of_text_is_the_elements_own_text() {
    let frame = describe_frame(
        &[
            open(1, Semantics::group()),
            text("Hello"),
            DrawCommand::PopElement,
        ],
        None,
        &mut Fixed,
        false,
    );
    let node = only_box(&frame);
    assert!(matches!(&node.content, Content::Text { text, runs: None } if text == "Hello"));
    assert!(node.style.contains("font-size:14px;"), "{}", node.style);
}

#[test]
fn two_runs_of_text_are_pieces_placed_inside_their_box() {
    let frame = describe_frame(
        &[
            open(1, Semantics::group()),
            text("one"),
            text("two"),
            DrawCommand::PopElement,
        ],
        None,
        &mut Fixed,
        false,
    );
    let node = only_box(&frame);
    let Content::Children { boxes, pieces } = &node.content else {
        panic!("the box holds pieces");
    };
    assert!(boxes.is_empty());
    assert_eq!(pieces.len(), 2);
    assert!(pieces[0].style.starts_with("position:absolute;"));
    assert!(
        node.style.contains("position:absolute;"),
        "a layout root is positioned already: {}",
        node.style
    );
    assert!(!node.style.contains("position:relative;"));
}

#[test]
fn boxes_keep_the_order_they_were_drawn_in() {
    let frame = describe_frame(
        &[
            open(1, Semantics::group()),
            open(2, Semantics::group()),
            DrawCommand::PopElement,
            open(3, Semantics::group()),
            DrawCommand::PopElement,
            DrawCommand::PopElement,
        ],
        None,
        &mut Fixed,
        false,
    );
    let Content::Children { boxes, .. } = &only_box(&frame).content else {
        panic!("the box holds boxes");
    };
    let ids: Vec<u64> = boxes.iter().map(|node| node.id).collect();
    assert_eq!(ids, [2, 3]);
}

#[test]
fn a_box_inside_a_drawing_is_part_of_its_picture() {
    let frame = describe_frame(
        &[
            open(1, Semantics::drawing().with_label("chart")),
            open(2, Semantics::group()),
            DrawCommand::PopElement,
            DrawCommand::PopElement,
        ],
        None,
        &mut Fixed,
        false,
    );
    let node = only_box(&frame);
    assert_eq!(node.tag, "svg");
    assert_eq!(node.described.role, Some("img"));
    assert!(matches!(&node.content, Content::Drawing(markup) if markup.contains("<g transform")));
}

#[test]
fn every_attribute_is_named_in_the_order_it_is_written() {
    let described = Described {
        role: Some("group"),
        label: Some("Card".to_string()),
        ..Described::default()
    };
    let names: Vec<&str> = described
        .attributes(5)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(
        names,
        [
            "role",
            "aria-label",
            "href",
            "target",
            "rel",
            "lang",
            "id",
            "aria-hidden",
            "aria-checked",
            "aria-pressed",
            "aria-selected",
            "aria-expanded",
            "aria-disabled",
            "tabindex",
            CONSUMED_KEYS_ATTRIBUTE,
            FOCUS_BOX_ATTRIBUTE,
        ]
    );
}

#[test]
fn a_picture_is_named_by_its_alt_even_when_nobody_named_it() {
    let described = Described {
        picture: true,
        ..Described::default()
    };
    let attributes = described.attributes(5);
    assert!(attributes.contains(&("alt", Some(String::new()))));
    assert!(!attributes.iter().any(|(name, _)| *name == "aria-label"));
}

/// The attributes a box with `toggled` set ends up with, by role, leaving out the ones that are absent.
fn state_attributes(role: Role, on: bool) -> (&'static str, Vec<(&'static str, String)>) {
    let frame = describe_frame(
        &[
            open(1, Semantics::of(role).in_state(false, Some(on), false)),
            DrawCommand::PopElement,
        ],
        None,
        &mut Fixed,
        false,
    );
    let node = only_box(&frame);
    let states = node
        .described
        .attributes(node.id)
        .into_iter()
        .filter(|(name, _)| {
            matches!(
                *name,
                "aria-checked" | "aria-pressed" | "aria-selected" | "aria-expanded"
            )
        })
        .filter_map(|(name, value)| Some((name, value?)))
        .collect();
    (node.tag, states)
}

/// One flag, written as the attribute ARIA has for the role: a switch is checked, a toggle button pressed, a tab selected, a disclosure expanded.
#[test]
fn a_state_is_the_attribute_its_role_carries_it_in() {
    let on = |name: &'static str| vec![(name, "true".to_string())];
    assert_eq!(
        state_attributes(Role::Switch, true),
        ("div", on("aria-checked"))
    );
    assert_eq!(
        state_attributes(Role::Button, false),
        ("button", vec![("aria-pressed", "false".to_string())])
    );
    assert_eq!(
        state_attributes(Role::Tab, true),
        ("div", on("aria-selected"))
    );
    assert_eq!(
        state_attributes(Role::Disclosure, true),
        ("div", on("aria-expanded"))
    );
    assert_eq!(
        state_attributes(Role::Slider, true),
        ("div", vec![]),
        "a role with no on/off state carries none"
    );
}
