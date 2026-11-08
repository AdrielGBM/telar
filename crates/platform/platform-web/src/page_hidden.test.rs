//! Hearing the page go out of view, against a real browser.

#![cfg(target_arch = "wasm32")]

use std::cell::Cell;
use std::rc::Rc;

use telar_platform_web::PageHidden;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

fn page_hides() {
    let event = web_sys::Event::new("pagehide").unwrap();
    web_sys::window().unwrap().dispatch_event(&event).unwrap();
}

#[wasm_bindgen_test]
fn a_page_leaving_is_heard_until_the_listener_goes() {
    let heard = Rc::new(Cell::new(0));
    let counting = heard.clone();
    let hidden = PageHidden::new(move || counting.set(counting.get() + 1));
    page_hides();
    assert_eq!(heard.get(), 1);
    drop(hidden);
    page_hides();
    assert_eq!(heard.get(), 1);
}

#[wasm_bindgen_test]
fn a_visibility_change_counts_only_when_it_leaves_the_page_hidden() {
    let heard = Rc::new(Cell::new(0));
    let counting = heard.clone();
    let _hidden = PageHidden::new(move || counting.set(counting.get() + 1));
    let event = web_sys::Event::new("visibilitychange").unwrap();
    let document = web_sys::window().unwrap().document().unwrap();
    let expected = u32::from(document.hidden());
    document.dispatch_event(&event).unwrap();
    assert_eq!(heard.get(), expected);
}
