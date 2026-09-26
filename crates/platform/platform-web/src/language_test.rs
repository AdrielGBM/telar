//! `<html lang>`/`<html dir>` following the active locale, against a real browser.

#![cfg(target_arch = "wasm32")]

use telar_platform_web::set_document_language;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

fn root() -> web_sys::Element {
    web_sys::window()
        .unwrap()
        .document()
        .unwrap()
        .document_element()
        .unwrap()
}

#[wasm_bindgen_test]
fn switching_to_an_ltr_locale_sets_lang_and_dir() {
    set_document_language("es", false);
    let root = root();
    assert_eq!(root.get_attribute("lang").as_deref(), Some("es"));
    assert_eq!(root.get_attribute("dir").as_deref(), Some("ltr"));
}

#[wasm_bindgen_test]
fn switching_to_an_rtl_locale_flips_dir() {
    set_document_language("ar", true);
    let rtl = root();
    assert_eq!(rtl.get_attribute("lang").as_deref(), Some("ar"));
    assert_eq!(rtl.get_attribute("dir").as_deref(), Some("rtl"));

    set_document_language("en", false);
    let back_to_ltr = root();
    assert_eq!(back_to_ltr.get_attribute("lang").as_deref(), Some("en"));
    assert_eq!(back_to_ltr.get_attribute("dir").as_deref(), Some("ltr"));
}
