use std::sync::Arc;

use geometry_core::Rect;
use platform_core::Location;
use renderer_core::{
    Color, Declared, Destination, DrawCommand, Element, ElementId, RectStyle, Role, Semantics,
    Span, TextStyle,
};

use super::*;

fn open(id: u64, semantics: Semantics, layout: &str, rect: Rect) -> DrawCommand {
    DrawCommand::PushElement {
        element: Arc::new(Element::new(ElementId(id), semantics, layout, rect)),
    }
}

fn text(content: &str, rect: Rect) -> DrawCommand {
    DrawCommand::Text {
        text: Arc::from(content),
        spans: None,
        rect,
        style: Arc::new(TextStyle::new(16.0, Color::BLACK)),
    }
}

fn page() -> Vec<DrawCommand> {
    vec![
        open(
            7,
            Semantics::of(Role::Main),
            "display:flex;flex-direction:column;",
            Rect::new(0.0, 0.0, 800.0, 600.0),
        ),
        open(
            8,
            Semantics::of(Role::Heading(1)),
            "",
            Rect::new(0.0, 0.0, 800.0, 40.0),
        ),
        text("Tom & <Jerry>", Rect::new(0.0, 0.0, 800.0, 40.0)),
        DrawCommand::PopElement,
        open(
            9,
            Semantics::of(Role::Link)
                .linking_to(Destination::Route(Location::root().segment("projects"))),
            "",
            Rect::new(0.0, 40.0, 100.0, 20.0),
        ),
        text("Projects", Rect::new(0.0, 40.0, 100.0, 20.0)),
        DrawCommand::PopElement,
        DrawCommand::PopElement,
    ]
}

#[test]
fn every_box_is_the_element_its_role_is_and_names_the_box_it_stands_for() {
    let written = prerender(&page(), None);
    assert!(
        written
            .markup
            .starts_with("<main data-telar-id=\"7\" style=\""),
        "{}",
        written.markup
    );
    assert!(
        written
            .markup
            .contains("<h1 data-telar-id=\"8\" style=\"font-size:16px;"),
        "{}",
        written.markup
    );
    assert!(
        written
            .markup
            .contains("<a data-telar-id=\"9\" href=\"/projects\" tabindex=\"-1\" style="),
        "a link is an `<a href>` before any code runs: {}",
        written.markup
    );
    assert!(
        written.markup.ends_with("</a></main>"),
        "{}",
        written.markup
    );
}

#[test]
fn text_is_escaped_as_text() {
    let written = prerender(&page(), None);
    assert!(
        written.markup.contains(">Tom &amp; &lt;Jerry&gt;</h1>"),
        "{}",
        written.markup
    );
}

#[test]
fn a_layout_root_is_placed_against_the_host_at_its_computed_size() {
    let written = prerender(&page(), None);
    let style = written
        .markup
        .split("style=\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .unwrap_or_default();
    assert_eq!(
        style,
        "display:flex;flex-direction:column;position:absolute;left:0px;top:0px;width:800px;height:600px;"
    );
}

#[test]
fn the_host_is_marked_as_the_apps_and_carries_the_surface_colour() {
    let written = prerender(&page(), Some(Color::rgb(0.0, 0.0, 0.0)));
    assert_eq!(
        written.host_attributes,
        vec![
            ("data-telar".to_string(), String::new()),
            (
                "style".to_string(),
                "position:relative;background-color:#000000;color-scheme:dark;".to_string()
            ),
        ]
    );
    assert_eq!(
        written.host_attributes_html(),
        " data-telar=\"\" style=\"position:relative;background-color:#000000;color-scheme:dark;\""
    );
}

#[test]
fn a_transparent_surface_names_no_colour() {
    let written = prerender(&page(), Some(Color::rgba(1.0, 1.0, 1.0, 0.0)));
    assert_eq!(written.host_attributes[1].1, "position:relative;");
}

#[test]
fn the_primary_scroll_is_the_documents_own_and_grows_with_its_content() {
    let root = Element::new(
        ElementId(3),
        Semantics::of(Role::ScrollArea),
        "",
        Rect::new(0.0, 0.0, 400.0, 300.0),
    )
    .as_primary_scroll(true);
    let commands = [
        DrawCommand::PushElement {
            element: Arc::new(root),
        },
        DrawCommand::PopElement,
        DrawCommand::Rect {
            rect: Rect::new(10.0, 20.0, 30.0, 40.0),
            style: Arc::new(RectStyle::filled(Color::BLACK, 0.0)),
        },
    ];
    let written = prerender(&commands, None);
    assert_eq!(
        written.host_attributes[1],
        ("data-telar-document-scroll".to_string(), String::new())
    );
    assert!(
        written.host_attributes[2]
            .1
            .contains("height:auto;touch-action:pan-x pan-y;overflow-x:clip;"),
        "{:?}",
        written.host_attributes
    );
    assert!(
        written
            .markup
            .starts_with("<div data-telar-id=\"3\" style=\"min-height:300px;\"></div>"),
        "{}",
        written.markup
    );
    assert!(
        written
            .markup
            .contains("<div style=\"position:fixed;left:10px;top:20px;width:30px;height:40px;"),
        "paint over a scrolling page stays put as it scrolls: {}",
        written.markup
    );
}

#[test]
fn paint_inside_a_box_is_a_positioned_piece_out_of_the_accessibility_tree() {
    let commands = [
        open(1, Semantics::group(), "", Rect::new(0.0, 0.0, 200.0, 200.0)),
        open(2, Semantics::group(), "", Rect::new(0.0, 0.0, 100.0, 100.0)),
        DrawCommand::Rect {
            rect: Rect::new(5.0, 5.0, 2.0, 10.0),
            style: Arc::new(RectStyle::filled(Color::BLACK, 0.0)),
        },
        DrawCommand::PopElement,
        DrawCommand::PopElement,
    ];
    let written = prerender(&commands, None);
    assert!(
        written.markup.contains(
            "<div data-telar-id=\"2\" style=\"position:relative;\"><div role=\"presentation\" style=\"position:absolute;left:5px;top:5px;width:2px;height:10px;"
        ),
        "the piece is placed against the box it was painted in: {}",
        written.markup
    );
}

#[test]
fn a_linked_span_is_an_anchor_naming_its_box_and_its_run() {
    let commands = [
        open(4, Semantics::group(), "", Rect::new(0.0, 0.0, 200.0, 20.0)),
        DrawCommand::Text {
            text: Arc::from("Read the docs"),
            spans: Some(Arc::from(vec![
                Span::new(9..13, Declared::default())
                    .linking_to(Destination::external("https://example.com").unwrap()),
            ])),
            rect: Rect::new(0.0, 0.0, 200.0, 20.0),
            style: Arc::new(TextStyle::new(16.0, Color::BLACK)),
        },
        DrawCommand::PopElement,
    ];
    let written = prerender(&commands, None);
    assert!(
        written.markup.contains(
            ">Read the <a href=\"https://example.com\" target=\"_blank\" rel=\"noopener\" data-telar-run-box=\"4\" data-telar-run=\"0\">docs</a></div>"
        ),
        "{}",
        written.markup
    );
}

#[test]
fn a_named_box_that_has_no_element_of_its_own_says_it_is_a_group() {
    let commands = [
        open(
            2,
            Semantics::group().with_label("Card \"one\""),
            "",
            Rect::new(0.0, 0.0, 10.0, 10.0),
        ),
        DrawCommand::PopElement,
    ];
    let written = prerender(&commands, None);
    assert!(
        written.markup.starts_with(
            "<div data-telar-id=\"2\" role=\"group\" aria-label=\"Card &quot;one&quot;\""
        ),
        "{}",
        written.markup
    );
}

#[test]
fn the_reset_is_the_stylesheet_the_running_app_would_install() {
    let sheet = reset_stylesheet();
    assert!(sheet.starts_with("<style id=\"telar-reset\">[data-telar]{"));
    assert!(sheet.ends_with("</style>"));
}
