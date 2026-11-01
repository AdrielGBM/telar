//! A text case, an underline and a backdrop blur in the document: CSS the page draws, measured the way the page draws it. Needs a browser, so it runs under `wasm-bindgen-test` rather than `cargo test`.

#![cfg(target_arch = "wasm32")]

use std::sync::Arc;

use geometry_core::Rect;
use renderer_core::{
    Color, DrawCommand, Element, ElementId, RectStyle, RenderBackend, Semantics, ShapeStyle,
    TextCase, TextMetrics, TextStyle,
};
use telar_renderer_dom::{CanvasTextMetrics, DomRenderer};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

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
    document()
        .body()
        .expect("a body")
        .append_child(host.as_ref())
        .expect("the host went into the page");
    host
}

fn computed(element: &web_sys::Element, property: &str) -> String {
    web_sys::window()
        .unwrap()
        .get_computed_style(element)
        .unwrap()
        .expect("a computed style")
        .get_property_value(property)
        .unwrap_or_default()
}

/// How wide the browser lays `text` out in 20px sans-serif with `css` applied, in a span nothing else styles, in the language `lang`.
fn drawn_width(text: &str, css: &str, lang: &str) -> f32 {
    let span: web_sys::HtmlElement = document()
        .create_element("span")
        .unwrap()
        .dyn_into()
        .unwrap();
    span.set_attribute(
        "style",
        &format!("position:absolute;white-space:pre;font:20px sans-serif;{css}"),
    )
    .unwrap();
    span.set_attribute("lang", lang).unwrap();
    span.set_text_content(Some(text));
    document().body().unwrap().append_child(&span).unwrap();
    let width = span.get_bounding_client_rect().width() as f32;
    span.remove();
    width
}

#[wasm_bindgen_test]
fn a_cased_text_is_measured_as_the_page_draws_it() {
    let metrics = CanvasTextMetrics;
    for (text, case, lang) in [
        ("menu straße", TextCase::Upper, "en"),
        ("istanbul", TextCase::Upper, "tr"),
        ("ABOUT THE WORK", TextCase::Lower, "en"),
        ("about the work", TextCase::Capitalize, "en"),
    ] {
        let style = TextStyle::new(20.0, Color::BLACK)
            .with_text_case(case)
            .with_lang(lang);
        let (measured, _) = metrics.measure(text, None, 10_000.0, &style);
        let drawn = drawn_width(text, &format!("text-transform:{}", case.css_name()), lang);
        assert!(
            (measured - drawn).abs() < 1.0,
            "{text} ({lang}, {case:?}): Telar measured {measured}, the page drew {drawn}"
        );
        let (written, _) =
            metrics.measure(text, None, 10_000.0, &TextStyle::new(20.0, Color::BLACK));
        assert!(
            (measured - written).abs() >= 1.0,
            "{text}: the case changed nothing measurable, so this checks nothing"
        );
    }
}

fn element(id: u64, rect: Rect) -> Arc<Element> {
    Arc::new(Element::new(ElementId(id), Semantics::group(), "", rect))
}

/// The element the paragraph `text` was written into, by what it holds.
fn paragraph(host: &web_sys::HtmlElement, text: &str) -> web_sys::Element {
    let all = host.query_selector_all("*").expect("a query");
    (0..all.length())
        .filter_map(|i| all.item(i)?.dyn_into::<web_sys::Element>().ok())
        .find(|element| {
            element.child_element_count() == 0 && element.text_content().as_deref() == Some(text)
        })
        .expect("the paragraph is in the page")
}

#[wasm_bindgen_test]
fn an_underline_and_a_case_are_css_and_the_page_keeps_the_written_text() {
    let host = host();
    let mut renderer = DomRenderer::new(host.clone()).expect("a renderer on the host");
    let style = TextStyle::new(16.0, Color::BLACK)
        .with_text_case(TextCase::Upper)
        .with_underline(true)
        .with_underline_offset(4.0)
        .with_underline_thickness(2.0)
        .with_underline_color(Color::from_rgb_u8(255, 0, 0));
    renderer
        .render_frame(
            &[
                DrawCommand::PushElement {
                    element: element(1, Rect::new(0.0, 0.0, 200.0, 40.0)),
                },
                DrawCommand::Text {
                    text: Arc::from("menu"),
                    spans: None,
                    rect: Rect::new(0.0, 0.0, 200.0, 20.0),
                    style: Arc::new(style),
                },
                DrawCommand::PopElement,
            ],
            None,
        )
        .expect("the frame reconciled");

    let text = paragraph(&host, "menu");
    assert_eq!(computed(&text, "text-transform"), "uppercase");
    assert_eq!(computed(&text, "text-decoration-line"), "underline");
    assert_eq!(computed(&text, "text-underline-offset"), "4px");
    assert_eq!(computed(&text, "text-decoration-thickness"), "2px");
    assert_eq!(computed(&text, "text-decoration-color"), "rgb(255, 0, 0)");
    assert_eq!(computed(&text, "text-decoration-skip-ink"), "none");
    host.remove();
}

#[wasm_bindgen_test]
fn a_backdrop_blur_is_a_backdrop_filter_on_the_box_itself() {
    let host = host();
    let mut renderer = DomRenderer::new(host.clone()).expect("a renderer on the host");
    renderer
        .render_frame(
            &[
                DrawCommand::PushElement {
                    element: element(1, Rect::new(0.0, 0.0, 100.0, 100.0)),
                },
                DrawCommand::PushElement {
                    element: element(2, Rect::new(0.0, 0.0, 50.0, 50.0)),
                },
                DrawCommand::PushLayer {
                    opacity: 1.0,
                    backdrop_blur: 20.0,
                    blend: renderer_core::BlendMode::Normal,
                    mask: renderer_core::LayerMask::None,
                },
                DrawCommand::Rect {
                    rect: Rect::new(0.0, 0.0, 50.0, 50.0),
                    style: Arc::new(
                        RectStyle::default().with_fill(Color::rgba(1.0, 1.0, 1.0, 0.5)),
                    ),
                },
                DrawCommand::PopLayer,
                DrawCommand::PopElement,
                DrawCommand::PopElement,
            ],
            None,
        )
        .expect("the frame reconciled");

    let outer = host.first_element_child().expect("the outer box");
    let frosted = outer.first_element_child().expect("the frosted box");
    assert_eq!(computed(&frosted, "backdrop-filter"), "blur(10px)");
    assert!(
        computed(&outer, "backdrop-filter") == "none",
        "only the box that asked for it blurs"
    );
    host.remove();
}
