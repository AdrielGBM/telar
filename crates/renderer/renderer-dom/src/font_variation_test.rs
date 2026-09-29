//! Text set on a face's variation axes is measured in the shape the page draws it in: a canvas cannot be told about an axis other than weight, so those are measured by the browser laying the text out. Needs a browser, so it runs under `wasm-bindgen-test` rather than `cargo test`.

#![cfg(target_arch = "wasm32")]

use renderer_core::{Color, FontFamily, FontVariations, TextMetrics, TextStyle};
use telar_renderer_dom::CanvasTextMetrics;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

const FACE: &[u8] = include_bytes!("../../renderer-text/test-fonts/TelarTest.ttf");
const FAMILY: &str = "Telar Variable Face";
const TEXT: &str = "Wide Words";

async fn load_face() {
    let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(FACE));
    let blob = web_sys::Blob::new_with_u8_array_sequence(&parts).expect("a blob");
    let url = web_sys::Url::create_object_url_with_blob(&blob).expect("a blob url");
    let face = web_sys::FontFace::new_with_str(FAMILY, &format!("url({url})")).expect("a face");
    let document = web_sys::window().unwrap().document().unwrap();
    document.fonts().add(&face).expect("added to the page");
    JsFuture::from(face.load().expect("a load"))
        .await
        .expect("the face loads");
}

/// How wide the browser lays `TEXT` out with `settings` applied, in a span nothing else styles.
fn drawn_width(settings: &str) -> f32 {
    let document = web_sys::window().unwrap().document().unwrap();
    let span: web_sys::HtmlElement = document.create_element("span").unwrap().dyn_into().unwrap();
    span.set_attribute(
        "style",
        &format!("position:absolute;white-space:pre;font:24px \"{FAMILY}\";{settings}"),
    )
    .unwrap();
    span.set_text_content(Some(TEXT));
    document.body().unwrap().append_child(&span).unwrap();
    let width = span.get_bounding_client_rect().width() as f32;
    span.remove();
    width
}

fn style(axes: FontVariations) -> TextStyle {
    TextStyle::new(24.0, Color::BLACK)
        .with_font_family(FontFamily::Named(FAMILY.into()))
        .with_font_variations(axes)
}

#[wasm_bindgen_test]
async fn an_axis_a_canvas_cannot_set_is_measured_as_the_page_draws_it() {
    load_face().await;
    let metrics = CanvasTextMetrics;
    for (axes, css) in [
        (
            FontVariations::new().with("opsz", 8.0),
            "font-variation-settings:\"opsz\" 8",
        ),
        (
            FontVariations::new().with("opsz", 72.0),
            "font-variation-settings:\"opsz\" 72",
        ),
        (
            FontVariations::new().with("wght", 900.0).with("opsz", 72.0),
            "font-variation-settings:\"opsz\" 72, \"wght\" 900",
        ),
    ] {
        let (measured, _) = metrics.measure(TEXT, None, 10_000.0, &style(axes));
        let drawn = drawn_width(css);
        assert!(
            (measured - drawn).abs() < 1.0,
            "{css}: Telar measured {measured}, the page drew {drawn}"
        );
    }
}
