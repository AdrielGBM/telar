//! The element a browser types into stands at the caret of a wrapped text area — on the row the caret is on, not over the top of the area — so an input method opens its candidates beside the text being composed.
//!
//! Needs a browser, so it runs under `wasm-bindgen-test` rather than `cargo test` — see the crate README.

#![cfg(target_arch = "wasm32")]

use geometry_core::Size;
use layout_core::LayoutStyle;
use platform_core::Event;
use reactive_core::signal;
use renderer_core::{Color, DrawCommand, RenderBackend, TextStyle, TextWrap};
use telar_renderer_dom::{CanvasTextMetrics, DomRenderer};
use ui_core::{ComponentList, Container, TextArea, WindowRoot, box_item};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

const WIDTH: f32 = 160.0;
const HEIGHT: f32 = 400.0;
const PARAGRAPH: &str =
    "the quick brown fox jumps over the lazy dog and then keeps on running far away";

fn document() -> web_sys::Document {
    web_sys::window()
        .and_then(|window| window.document())
        .expect("a document")
}

#[wasm_bindgen_test]
fn the_entry_stands_on_the_row_a_wrapped_area_has_its_caret_on() {
    renderer_core::set_text_metrics(CanvasTextMetrics);
    ui_tree::set_element_capture(true);
    ui_core::set_surface_size(Size::new(WIDTH, HEIGHT));
    let style = || TextStyle::new(16.0, Color::BLACK);
    let area = TextArea::new(signal(PARAGRAPH.to_string()), LayoutStyle::new(), style)
        .expect("the area builds")
        .text_wrap(TextWrap::Wrap);
    let focus = area.focus_handle();
    let column = Container::new(
        LayoutStyle::new().flex_column().width(WIDTH),
        vec![box_item(area)],
    )
    .expect("the column builds");
    let mut tree = ComponentList::new(WindowRoot::new(box_item(column)));
    tree.on_event(&Event::WindowResized {
        width: WIDTH as u32,
        height: HEIGHT as u32,
    });
    let host: web_sys::HtmlElement = document()
        .create_element("div")
        .expect("a host element")
        .dyn_into()
        .expect("a div is an HTML element");
    host.style()
        .set_property("width", &format!("{WIDTH}px"))
        .expect("the host takes a width");
    document()
        .body()
        .expect("a body")
        .append_child(host.as_ref())
        .expect("the host went into the page");
    let mut renderer = DomRenderer::new(host.clone()).expect("a renderer on the host");
    ui_core::relayout_if_dirty();
    focus.request();
    let frame = tree.commands().clone();
    renderer
        .render_frame(&frame, None)
        .expect("the frame reconciled");

    let caret = frame
        .iter()
        .find_map(|command| match command {
            DrawCommand::PushElement { element } => element.caret,
            _ => None,
        })
        .expect("the focused area says where its caret is");
    let line = ui_core::line_box(&style());
    assert!(
        caret.y >= line,
        "the caret is past the first row: {caret:?}"
    );
    let entry = host
        .query_selector("textarea[data-telar-entry]")
        .expect("a valid selector")
        .expect("the entry is in the host")
        .get_bounding_client_rect();
    let origin = host.get_bounding_client_rect();
    assert!(
        ((entry.top() - origin.top()) as f32 - caret.y).abs() < 1.0,
        "the entry is at {} for a caret at {}",
        entry.top() - origin.top(),
        caret.y
    );
    assert!(((entry.left() - origin.left()) as f32 - caret.x).abs() < 1.0);
    assert!((entry.height() as f32 - caret.height).abs() < 1.0);
    host.remove();
}
