//! A text area that wraps places its caret by the rows the document's measurer breaks it into, while the browser draws the text and breaks it itself. The two have to break at the same letters, or the caret stands on one row and the word it is in on another.
//!
//! Needs a browser, so it runs under `wasm-bindgen-test` rather than `cargo test` — see the crate README.

#![cfg(target_arch = "wasm32")]

use geometry_core::Size;
use layout_core::LayoutStyle;
use platform_core::Event;
use reactive_core::signal;
use renderer_core::{Color, RenderBackend, TextStyle, TextWrap};
use telar_renderer_dom::{CanvasTextMetrics, DomRenderer, ID_ATTRIBUTE};
use ui_core::{ComponentList, Container, LayoutItem, TextArea, WindowRoot, box_item};
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

fn style() -> TextStyle {
    TextStyle::new(16.0, Color::BLACK)
}

/// The first text node under `node` holding `text`.
fn text_node(node: &web_sys::Node, text: &str) -> Option<web_sys::Node> {
    if node.node_type() == web_sys::Node::TEXT_NODE {
        return node
            .text_content()
            .is_some_and(|content| content == text)
            .then(|| node.clone());
    }
    let children = node.child_nodes();
    (0..children.length())
        .filter_map(|index| children.item(index))
        .find_map(|child| text_node(&child, text))
}

/// The top of the line the browser drew the character at `at` on.
fn top_of(node: &web_sys::Node, at: usize) -> f64 {
    let range = document().create_range().expect("a range");
    range
        .set_start(node, at as u32)
        .expect("a start in the text");
    range
        .set_end(node, at as u32 + 1)
        .expect("an end in the text");
    range.get_bounding_client_rect().top()
}

#[wasm_bindgen_test]
fn the_browser_breaks_a_wrapped_text_area_where_its_caret_does() {
    renderer_core::set_text_metrics(CanvasTextMetrics);
    ui_tree::set_element_capture(true);
    ui_core::set_surface_size(Size::new(WIDTH, HEIGHT));
    let area = TextArea::new(signal(PARAGRAPH.to_string()), LayoutStyle::new(), style)
        .expect("the area builds")
        .text_wrap(TextWrap::Wrap);
    let area_node = area.layout_node();
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
    let frame = tree.commands().clone();
    renderer
        .render_frame(&frame, None)
        .expect("the frame reconciled");

    let element = host
        .query_selector(&format!("[{ID_ATTRIBUTE}=\"{}\"]", u64::from(area_node)))
        .expect("a valid selector")
        .expect("the area is in the document");
    let text = text_node(element.as_ref(), PARAGRAPH).expect("the area draws its text");
    let starts = renderer_core::text_line_starts(
        PARAGRAPH,
        None,
        WIDTH,
        &style().with_text_wrap(TextWrap::Wrap),
    );
    assert!(starts.len() >= 3, "{starts:?}");
    let line = ui_core::line_box(&style());
    let area_height = ui_core::track_layout(area_node)
        .expect("the area is laid out")
        .get()
        .height;
    assert!(
        (area_height - starts.len() as f32 * line).abs() < 1.0,
        "the area is {area_height} tall for {} rows",
        starts.len()
    );
    for pair in starts.windows(2) {
        let (row, next) = (pair[0], pair[1]);
        assert_eq!(
            top_of(&text, row),
            top_of(&text, next - 2),
            "the browser keeps {:?} on one row",
            &PARAGRAPH[row..next]
        );
        assert!(
            top_of(&text, next) > top_of(&text, row),
            "the browser breaks before {:?}",
            &PARAGRAPH[next..]
        );
    }
    host.remove();
}
