//! A dashed stroke in the document: the drawing's `<path>` and `<line>` carry `stroke-dasharray` and `stroke-dashoffset`, and the browser reads them back as the pattern Telar asked for.
//!
//! Needs a browser, so it runs under `wasm-bindgen-test` rather than `cargo test` — see the crate README.

#![cfg(target_arch = "wasm32")]

use std::sync::Arc;

use geometry_core::{Point, Rect};
use renderer_core::{
    Color, DrawCommand, Element, ElementId, PathData, PathStyle, RenderBackend, Semantics, Stroke,
};
use telar_renderer_dom::DomRenderer;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

fn window() -> web_sys::Window {
    web_sys::window().expect("a window")
}

fn host() -> web_sys::HtmlElement {
    let document = window().document().expect("a document");
    let host: web_sys::HtmlElement = document
        .create_element("div")
        .expect("a host element")
        .dyn_into()
        .expect("a div is an HTML element");
    document
        .body()
        .expect("a body")
        .append_child(host.as_ref())
        .expect("the host went into the page");
    host
}

fn drawn(stroke: Stroke) -> Vec<DrawCommand> {
    let path = PathData::new()
        .move_to(Point::new(0.0, 10.0))
        .line_to(Point::new(100.0, 10.0));
    vec![
        DrawCommand::PushElement {
            element: Arc::new(Element::new(
                ElementId(1),
                Semantics::drawing(),
                "",
                Rect::new(0.0, 0.0, 100.0, 40.0),
            )),
        },
        DrawCommand::Path {
            data: Arc::new(path),
            style: Arc::new(PathStyle::default().with_stroke(stroke)),
        },
        DrawCommand::Line {
            p1: Point::new(0.0, 30.0),
            p2: Point::new(100.0, 30.0),
            style: Arc::new(stroke),
        },
        DrawCommand::PopElement,
    ]
}

fn computed(element: &web_sys::Element, property: &str) -> String {
    window()
        .get_computed_style(element)
        .expect("computed style")
        .expect("a style for an element in the page")
        .get_property_value(property)
        .expect("a property value")
}

fn shapes(host: &web_sys::HtmlElement) -> Vec<web_sys::Element> {
    let found = host
        .query_selector_all("path, line")
        .expect("a valid selector");
    (0..found.length())
        .filter_map(|i| found.item(i))
        .filter_map(|node| node.dyn_into::<web_sys::Element>().ok())
        .collect()
}

#[wasm_bindgen_test]
fn a_dashed_stroke_reaches_the_browser_as_its_pattern_and_offset() {
    let host = host();
    let mut renderer = DomRenderer::new(host.clone()).expect("a renderer on the host");
    let stroke = Stroke::new(Color::BLACK, 1.0).with_dash(&[1.0, 4.0], 2.0);
    renderer
        .render_frame(&drawn(stroke), None)
        .expect("the frame reconciled");

    let shapes = shapes(&host);
    assert_eq!(shapes.len(), 2, "{}", host.inner_html());
    for shape in &shapes {
        assert_eq!(
            shape.get_attribute("stroke-dasharray").as_deref(),
            Some("1 4")
        );
        assert_eq!(
            shape.get_attribute("stroke-dashoffset").as_deref(),
            Some("2")
        );
        let pattern: Vec<String> = computed(shape, "stroke-dasharray")
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|part| !part.is_empty())
            .map(|part| part.trim_end_matches("px").to_string())
            .collect();
        assert_eq!(pattern, ["1", "4"], "the browser reads the pattern back");
        assert_eq!(
            computed(shape, "stroke-dashoffset").trim_end_matches("px"),
            "2",
            "the browser reads the offset back"
        );
    }
    host.remove();
}

#[wasm_bindgen_test]
fn a_solid_stroke_stays_solid_in_the_browser() {
    let host = host();
    let mut renderer = DomRenderer::new(host.clone()).expect("a renderer on the host");
    renderer
        .render_frame(&drawn(Stroke::new(Color::BLACK, 1.0)), None)
        .expect("the frame reconciled");

    let shapes = shapes(&host);
    assert_eq!(shapes.len(), 2, "{}", host.inner_html());
    for shape in &shapes {
        assert!(shape.get_attribute("stroke-dasharray").is_none());
        assert_eq!(computed(shape, "stroke-dasharray"), "none");
    }
    host.remove();
}
