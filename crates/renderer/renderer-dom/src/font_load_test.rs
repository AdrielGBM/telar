//! A face the page loads after the first layout: the measurer has to forget what it measured in the fallback, once, when the face lands. Needs a browser, so it runs under `wasm-bindgen-test` rather than `cargo test`.

#![cfg(target_arch = "wasm32")]

use renderer_core::{Color, FontFamily, TextMetrics, TextStyle};
use telar_renderer_dom::{CanvasTextMetrics, remeasure_on_font_load};
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

const FACE: &[u8] = include_bytes!("../../renderer-text/test-fonts/TelarTest.ttf");
const FAMILY: &str = "Telar Late Face";

fn style() -> TextStyle {
    TextStyle::new(24.0, Color::BLACK).with_font_family(FontFamily::stack([
        FontFamily::Named(FAMILY.into()),
        FontFamily::Monospace,
    ]))
}

async fn pause() {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        let _ = web_sys::window()
            .expect("a window")
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 20);
    });
    let _ = JsFuture::from(promise).await;
}

/// Declares the face the way `@font-face` does — a URL the browser fetches — so the page's font set goes through a load.
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

#[wasm_bindgen_test]
async fn a_face_that_lands_is_measured_in_once_the_page_says_so() {
    remeasure_on_font_load();
    let metrics = CanvasTextMetrics;
    let fallback = metrics.measure("Wide Words", None, 1000.0, &style());
    let fallback_line = metrics.line_height(24.0);
    let generation = renderer_core::text_metrics_generation();

    load_face().await;
    let mut waited = 0;
    while renderer_core::text_metrics_generation() == generation && waited < 100 {
        pause().await;
        waited += 1;
    }

    assert!(
        renderer_core::text_metrics_generation() > generation,
        "the page's `loadingdone` must tell layout its measurements are stale"
    );
    let arrived = metrics.measure("Wide Words", None, 1000.0, &style());
    assert_ne!(
        arrived.0, fallback.0,
        "the text must measure in the face that landed, not the fallback"
    );
    assert!(metrics.line_height(24.0) > 0.0 && fallback_line > 0.0);
}
