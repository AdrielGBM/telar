use std::sync::Arc;

use geometry_core::Rect;
use renderer_core::{
    Border, Color, DrawCommand, Element, ElementId, ForcedColors, ImageData, Picture, RectStyle,
    Role, Semantics, TextStyle,
};

use super::*;

fn style_of(markup: &str, id: u64) -> String {
    let tag = format!("data-telar-id=\"{id}\"");
    markup
        .split(&tag)
        .nth(1)
        .and_then(|rest| rest.split("style=\"").nth(1))
        .and_then(|rest| rest.split('"').next())
        .unwrap_or_default()
        .to_string()
}

fn plain_element() -> Element {
    Element::new(
        ElementId(1),
        Semantics::group(),
        "",
        Rect::new(0.0, 0.0, 20.0, 20.0),
    )
}

fn alone(element: Element) -> Vec<DrawCommand> {
    vec![
        DrawCommand::PushElement {
            element: Arc::new(element),
        },
        DrawCommand::PopElement,
    ]
}

#[test]
fn colour_content_opts_out_of_forced_colors_and_other_boxes_do_not() {
    let swatch = alone(plain_element().with_forced_colors(ForcedColors::Keep));
    let markup = prerender(&swatch, None).markup;
    assert!(
        style_of(&markup, 1).contains("forced-color-adjust:none;"),
        "{markup}"
    );
    let markup = prerender(&alone(plain_element()), None).markup;
    assert!(!markup.contains("forced-color-adjust"), "{markup}");
}

#[test]
fn a_picture_keeps_its_colours_without_being_told_to() {
    let linked = ImageData::linked("a.png", 20, 20, &[]);
    let picture = Picture {
        source: linked.linked_source().expect("a linked picture").clone(),
        width: 20,
        height: 20,
        fit: renderer_core::PictureFit::Cover,
        priority: false,
    };
    let markup = prerender(&alone(plain_element().showing(picture)), None).markup;
    assert!(
        style_of(&markup, 1).contains("forced-color-adjust:none;"),
        "{markup}"
    );
}

fn labelled_box(style: RectStyle) -> Vec<DrawCommand> {
    let rect = Rect::new(0.0, 0.0, 100.0, 40.0);
    vec![
        DrawCommand::PushElement {
            element: Arc::new(Element::new(
                ElementId(1),
                Semantics::of(Role::Group),
                "",
                rect,
            )),
        },
        DrawCommand::Rect {
            rect,
            style: Arc::new(style),
        },
        DrawCommand::Text {
            text: Arc::from("Label"),
            spans: None,
            rect,
            style: Arc::new(TextStyle::new(16.0, Color::BLACK)),
        },
        DrawCommand::PopElement,
    ]
}

#[test]
fn a_one_sided_frame_leaves_the_content_box_of_the_element_unchanged() {
    let framed =
        RectStyle::default().with_border(Border::per_side(Color::BLACK, 0.0, 0.0, 2.0, 0.0));
    let markup = prerender(&labelled_box(framed), None).markup;
    let owner = style_of(&markup, 1);
    assert!(
        !owner.contains("border"),
        "the element takes no width from its content: {owner}"
    );
    assert!(
        markup.contains("border-width:0px 0px 2px 0px;"),
        "the frame is still drawn, as a piece over the box: {markup}"
    );
}

#[test]
fn a_keyboard_ring_names_the_highlight_on_the_element_and_on_its_inner_band() {
    let rect = Rect::new(0.0, 0.0, 100.0, 40.0);
    let ring = Border::uniform(Color::BLACK, 2.0).as_focus_ring();
    let commands = vec![
        DrawCommand::PushElement {
            element: Arc::new(Element::new(ElementId(1), Semantics::group(), "", rect)),
        },
        DrawCommand::Rect {
            rect,
            style: Arc::new(RectStyle::default().with_border(ring)),
        },
        DrawCommand::Rect {
            rect: Rect::new(2.0, 2.0, 96.0, 36.0),
            style: Arc::new(RectStyle::default().with_border(ring)),
        },
        DrawCommand::PopElement,
    ];
    let markup = prerender(&commands, None).markup;
    assert_eq!(
        markup.matches("--telar-ring:Highlight;").count(),
        2,
        "{markup}"
    );
}
