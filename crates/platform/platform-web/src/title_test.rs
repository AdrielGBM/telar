//! The surface's title as the page's own, against a real browser. Needs one, so it runs under `wasm-bindgen-test` rather than `cargo test`.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;

use platform_core::{Event, EventHandler, Platform, Window, WindowConfig};
use telar_platform_web::{WebPlatform, WebPlatformConfig, WebWindow};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

thread_local! {
    static WINDOW: RefCell<Option<WebWindow>> = const { RefCell::new(None) };
}

struct Keeper;

impl EventHandler<WebWindow> for Keeper {
    fn on_resume(&mut self, window: &WebWindow) -> bool {
        WINDOW.with(|kept| *kept.borrow_mut() = Some(window.clone()));
        true
    }

    fn on_event(&mut self, _event: Event, _window: &WebWindow) {}

    fn on_redraw(&mut self, _window: &WebWindow) {}
}

fn document() -> web_sys::Document {
    web_sys::window().unwrap().document().unwrap()
}

#[wasm_bindgen_test]
fn the_page_is_titled_as_the_window_opens_and_renamed_with_it() {
    let host: web_sys::HtmlElement = document()
        .create_element("div")
        .unwrap()
        .dyn_into()
        .unwrap();
    document().body().unwrap().append_child(&host).unwrap();
    let config = WebPlatformConfig {
        autofocus: false,
        ..WebPlatformConfig::default()
    };
    let window = WindowConfig {
        title: "Portfolio".to_string(),
        ..WindowConfig::default()
    };
    WebPlatform::with_host(host, config)
        .run(window, Keeper)
        .expect("the platform mounted");
    assert_eq!(document().title(), "Portfolio");

    let window = WINDOW
        .with(|kept| kept.borrow().clone())
        .expect("the platform resumed the handler on mount");
    window.set_title("Credits — Portfolio");
    assert_eq!(document().title(), "Credits — Portfolio");
    window.set_title("Créditos — Portafolio");
    assert_eq!(document().title(), "Créditos — Portafolio");
}
