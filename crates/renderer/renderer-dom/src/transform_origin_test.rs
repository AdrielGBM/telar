//! `transform_origin` (`box_transform_about`) in the document: a scaled box lands where its pivot says, in both writing directions.
//!
//! Needs a browser, so it runs under `wasm-bindgen-test` rather than `cargo test` — see the crate README.

#![cfg(target_arch = "wasm32")]

use std::sync::Arc;

use geometry_core::Rect;
use layout_core::Direction;
use renderer_core::{
    Color, DrawCommand, Element, ElementId, RectStyle, RenderBackend, Semantics, ShapeStyle,
};
use telar_renderer_dom::DomRenderer;
use ui_core::{TransformOrigin, box_transform_about, set_direction};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

const BOX: Rect = Rect {
    x: 200.0,
    y: 100.0,
    width: 100.0,
    height: 50.0,
};

fn document() -> web_sys::Document {
    web_sys::window()
        .and_then(|window| window.document())
        .expect("a document")
}

fn host() -> web_sys::HtmlElement {
    let host: web_sys::HtmlElement = document()
        .create_element("div")
        .expect("a host element")
        .dyn_into()
        .expect("a div is an HTML element");
    let _ = host.style().set_property("position", "relative");
    document()
        .body()
        .expect("a body")
        .append_child(host.as_ref())
        .expect("the host went into the page");
    host
}

/// Draws `BOX` under the matrix `origin` gives a 1.5x scale and returns where the browser put it, in the host's coordinates.
fn scaled_from(origin: TransformOrigin) -> Rect {
    let matrix = box_transform_about(BOX, origin, 0.0, 1.5, 1.5, 0.0, 0.0).expect("not identity");
    let commands = vec![
        DrawCommand::PushElement {
            element: Arc::new(Element::new(ElementId(1), Semantics::group(), "", BOX)),
        },
        DrawCommand::PushMatrix { matrix },
        DrawCommand::Rect {
            rect: BOX,
            style: Arc::new(RectStyle::default().with_fill(Color::WHITE)),
        },
        DrawCommand::PopMatrix,
        DrawCommand::PopElement,
    ];
    let host = host();
    let mut renderer = DomRenderer::new(host.clone()).expect("a renderer on the host");
    renderer
        .render_frame(&commands, None)
        .expect("the frame reconciled");
    let at = host.get_bounding_client_rect();
    let shown = host
        .first_element_child()
        .expect("the box")
        .get_bounding_client_rect();
    host.remove();
    Rect::new(
        (shown.x() - at.x()) as f32,
        (shown.y() - at.y()) as f32,
        shown.width() as f32,
        shown.height() as f32,
    )
}

fn assert_rect(case: &str, got: Rect, x: f32, y: f32, width: f32, height: f32) {
    for (edge, expected, got) in [
        ("x", x, got.x),
        ("y", y, got.y),
        ("width", width, got.width),
        ("height", height, got.height),
    ] {
        assert!(
            (expected - got).abs() < 0.5,
            "{case}: {edge} should be {expected}, the browser drew {got}"
        );
    }
}

#[wasm_bindgen_test]
fn the_default_pivot_grows_a_box_about_its_centre() {
    set_direction(Direction::Ltr);
    assert_rect(
        "centre",
        scaled_from(TransformOrigin::CENTER),
        175.0,
        87.5,
        150.0,
        75.0,
    );
}

#[wasm_bindgen_test]
fn start_pins_the_left_edge_in_left_to_right_text() {
    set_direction(Direction::Ltr);
    assert_rect(
        "ltr start",
        scaled_from(TransformOrigin::start()),
        200.0,
        87.5,
        150.0,
        75.0,
    );
}

#[wasm_bindgen_test]
fn start_pins_the_right_edge_in_right_to_left_text() {
    set_direction(Direction::Rtl);
    let shown = scaled_from(TransformOrigin::start());
    set_direction(Direction::Ltr);
    assert_rect("rtl start", shown, 150.0, 87.5, 150.0, 75.0);
}

#[wasm_bindgen_test]
fn a_pair_pins_the_corner_it_names() {
    set_direction(Direction::Ltr);
    assert_rect(
        "top end",
        scaled_from(TransformOrigin::new(1.0, 0.0)),
        150.0,
        100.0,
        150.0,
        75.0,
    );
}
