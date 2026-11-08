//! The safe area as the page's CSS says it, against a real browser.

#![cfg(target_arch = "wasm32")]

use telar_platform_web::safe_area_insets;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

/// A desktop browser keeps nothing for itself, and the probe that reads it stays out of sight and out of reach.
#[wasm_bindgen_test]
fn a_desktop_page_keeps_nothing_and_the_probe_stays_out_of_the_way() {
    assert_eq!(safe_area_insets(), geometry_core::Insets::default());
    assert_eq!(safe_area_insets(), geometry_core::Insets::default());
    let document = web_sys::window().unwrap().document().unwrap();
    let probe: web_sys::HtmlElement = document
        .get_element_by_id("telar-safe-area-probe")
        .expect("the probe is in the page")
        .dyn_into()
        .unwrap();
    assert_eq!(probe.get_attribute("aria-hidden").as_deref(), Some("true"));
    assert_eq!(probe.get_bounding_client_rect().width(), 0.0);
}
