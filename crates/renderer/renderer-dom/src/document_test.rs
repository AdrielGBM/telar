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

    fn host_origin(&mut self) -> (f32, f32) {
        (0.0, 0.0)
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
            "aria-current",
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

/// The `aria-current` a link marked current ends up with, `None` when it carries none.
fn current_attribute(semantics: Semantics) -> Option<String> {
    let frame = describe_frame(
        &[open(1, semantics), DrawCommand::PopElement],
        None,
        &mut Fixed,
        false,
    );
    let node = only_box(&frame);
    node.described
        .attributes(node.id)
        .into_iter()
        .find(|(name, _)| *name == "aria-current")
        .and_then(|(_, value)| value)
}

/// A route is the current page, an anchor the current location on it, and a language or an outside address just the current one; a link not marked, or one that goes nowhere, says nothing.
#[test]
fn a_current_link_says_what_it_is_the_current_one_of() {
    let link = |destination: Destination| Semantics::group().linking_to(destination);
    let route = Destination::Route(platform_core::Location::root().segment("about"));
    assert_eq!(
        current_attribute(link(route).marked_current(true)).as_deref(),
        Some("page")
    );
    assert_eq!(
        current_attribute(link(Destination::anchor("web")).marked_current(true)).as_deref(),
        Some("location")
    );
    assert_eq!(
        current_attribute(link(Destination::locale("en")).marked_current(true)).as_deref(),
        Some("true")
    );
    assert_eq!(current_attribute(link(Destination::anchor("web"))), None);
    assert_eq!(
        current_attribute(
            link(Destination::anchor("web"))
                .marked_current(true)
                .in_state(false, None, true)
        ),
        None,
        "a disabled link is no `<a href>`, so it is the current one of nothing"
    );
}

fn layer(id: u64, place: u64) -> DrawCommand {
    DrawCommand::PushElement {
        element: Arc::new(
            Element::new(
                ElementId(id),
                Semantics::group(),
                "display:flex;",
                Rect::new(0.0, 0.0, 800.0, 600.0),
            )
            .fixed_in_place_of(ElementId(place)),
        ),
    }
}

/// A page holding a layer's place (2) before a sticky stage (3), with the layer (10) and its bar (11) composed after the page, as a raster target draws them.
fn page_with_layer() -> Vec<DrawCommand> {
    vec![
        open(1, Semantics::group()),
        open(2, Semantics::group()),
        DrawCommand::PopElement,
        open(3, Semantics::group()),
        DrawCommand::PopElement,
        DrawCommand::PopElement,
        layer(10, 2),
        open(11, Semantics::group()),
        DrawCommand::PopElement,
        DrawCommand::PopElement,
    ]
}

/// Tab walks a document in the order of its elements, and so does a reader, so the layer goes where it was declared rather than after the page it is drawn over.
#[test]
fn a_layer_takes_the_place_it_was_declared_in() {
    let frame = describe_frame(&page_with_layer(), None, &mut Fixed, false);
    let page = only_box(&frame);
    let Content::Children { boxes, .. } = &page.content else {
        panic!("the page holds boxes");
    };
    let ids: Vec<u64> = boxes.iter().map(|node| node.id).collect();
    assert_eq!(ids, [10, 3], "the layer stands in for its place");
}

#[test]
fn a_layer_is_fixed_over_the_page_and_takes_the_pointer_only_over_its_boxes() {
    let frame = describe_frame(&page_with_layer(), None, &mut Fixed, false);
    let page = only_box(&frame);
    let Content::Children { boxes, .. } = &page.content else {
        panic!("the page holds boxes");
    };
    let layer = &boxes[0];
    for declaration in [
        "position:fixed;",
        "left:0px;",
        "top:0px;",
        "width:800px;",
        "height:600px;",
        "pointer-events:none;",
        "z-index:1;",
    ] {
        assert!(
            layer.style.contains(declaration),
            "{declaration} in {}",
            layer.style
        );
    }
    let Content::Children { boxes: bar, .. } = &layer.content else {
        panic!("the layer holds its bar");
    };
    assert!(
        bar[0].style.contains("pointer-events:auto;"),
        "{}",
        bar[0].style
    );
    assert!(
        page.style.contains("isolation:isolate;"),
        "the lift stays inside the root it is declared in, under what is placed after it: {}",
        page.style
    );
}

#[test]
fn a_layer_whose_place_is_missing_still_stands_over_the_surface() {
    let frame = describe_frame(
        &[
            open(1, Semantics::group()),
            DrawCommand::PopElement,
            layer(10, 99),
            DrawCommand::PopElement,
        ],
        None,
        &mut Fixed,
        false,
    );
    let ids: Vec<u64> = frame
        .children
        .iter()
        .map(|node| match node {
            Node::Box(node) => node.id,
            Node::Paint(_) => 0,
        })
        .collect();
    assert_eq!(ids, [1, 10]);
}

fn open_at(id: u64, semantics: Semantics, rect: Rect) -> DrawCommand {
    DrawCommand::PushElement {
        element: Arc::new(Element::new(ElementId(id), semantics, "", rect)),
    }
}

fn filled(rect: Rect) -> DrawCommand {
    DrawCommand::Rect {
        rect,
        style: Arc::new(renderer_core::RectStyle::filled(Color::WHITE, 0.0)),
    }
}

fn drawn(commands: &[DrawCommand]) -> String {
    let frame = describe_frame(commands, None, &mut Fixed, false);
    match &only_box(&frame).content {
        Content::Drawing(markup) => markup.clone(),
        _ => panic!("the box is a drawing"),
    }
}

/// A box scaled 1.5x from its start, 128px down a drawing that is itself 300px down the page: the scale is about (56, 468) on the surface, which is (0, 40) in the box.
#[test]
fn a_box_inside_a_drawing_transforms_and_cuts_itself_where_it_stands() {
    let at = Rect::new(56.0, 428.0, 200.0, 80.0);
    let markup = drawn(&[
        open_at(1, Semantics::drawing(), Rect::new(0.0, 300.0, 400.0, 200.0)),
        open_at(2, Semantics::group(), at),
        DrawCommand::PushClip {
            rect: at,
            radius: renderer_core::BorderRadius::zero(),
        },
        DrawCommand::PushMatrix {
            matrix: [1.5, 0.0, 0.0, 1.5, -28.0, -234.0],
        },
        filled(at),
        DrawCommand::PopMatrix,
        DrawCommand::PopClip,
        DrawCommand::PopElement,
        DrawCommand::PopElement,
    ]);
    assert!(
        markup.contains("<g transform=\"translate(56,128)\">"),
        "{markup}"
    );
    assert!(
        markup.contains("<rect x=\"0\" y=\"0\" width=\"200\" height=\"80\"/></clipPath>"),
        "{markup}"
    );
    assert!(
        markup.contains("<g transform=\"matrix(1.5,0,0,1.5,0,-20)\">"),
        "{markup}"
    );
    assert!(
        markup.contains("<rect x=\"0\" y=\"0\" width=\"200\" height=\"80\" fill=\"#ffffff\"/>"),
        "{markup}"
    );
}

/// What a canvas draws starts at its own corner, transforms included, wherever the canvas is on the page.
#[test]
fn artwork_inside_a_drawing_keeps_its_own_coordinates() {
    let markup = drawn(&[
        open_at(
            1,
            Semantics::drawing(),
            Rect::new(40.0, 300.0, 100.0, 100.0),
        ),
        DrawCommand::PushMatrix {
            matrix: [2.0, 0.0, 0.0, 2.0, 0.0, 0.0],
        },
        filled(Rect::new(10.0, 10.0, 20.0, 20.0)),
        DrawCommand::PopMatrix,
        DrawCommand::PushMatrix {
            matrix: [2.0, 0.0, 0.0, 2.0, 0.0, 0.0],
        },
        filled(Rect::new(0.0, 0.0, 100.0, 100.0)),
        DrawCommand::PopMatrix,
        DrawCommand::PopElement,
    ]);
    assert_eq!(
        markup,
        "<g transform=\"matrix(2,0,0,2,0,0)\"><rect x=\"10\" y=\"10\" width=\"20\" height=\"20\" fill=\"#ffffff\"/></g>\
<g transform=\"matrix(2,0,0,2,0,0)\"><rect x=\"0\" y=\"0\" width=\"100\" height=\"100\" fill=\"#ffffff\"/></g>"
    );
}

/// A frame drawn at the box's own corner says its matrix is in the box's coordinates too, so there is nothing to rebase.
#[test]
fn a_matrix_around_a_frame_at_the_corner_is_already_the_boxs_own() {
    let frame = describe_frame(
        &[
            open_at(1, Semantics::group(), Rect::new(200.0, 100.0, 100.0, 50.0)),
            DrawCommand::PushMatrix {
                matrix: [1.5, 0.0, 0.0, 1.5, 0.0, -12.5],
            },
            filled(Rect::new(0.0, 0.0, 100.0, 50.0)),
            DrawCommand::PopMatrix,
            DrawCommand::PopElement,
        ],
        None,
        &mut Fixed,
        false,
    );
    let node = only_box(&frame);
    assert!(
        node.style
            .contains("transform:matrix(1.5,0,0,1.5,0,-12.5);"),
        "{}",
        node.style
    );
}
