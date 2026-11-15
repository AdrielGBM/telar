//! Whether a Tab on a canvas surface stays the app's or goes to the page, against a real browser.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;

use platform_core::{Event, EventHandler, Platform, TabLeaves, Window, WindowConfig};
use telar_platform_web::{WebPlatform, WebPlatformConfig, WebWindow};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

thread_local! {
    static WINDOW: RefCell<Option<WebWindow>> = const { RefCell::new(None) };
}

struct Idle;

impl EventHandler<WebWindow> for Idle {
    fn on_resume(&mut self, window: &WebWindow) -> bool {
        WINDOW.with(|kept| *kept.borrow_mut() = Some(window.clone()));
        true
    }

    fn on_event(&mut self, _event: Event, _window: &WebWindow) {}

    fn on_redraw(&mut self, _window: &WebWindow) {}
}

fn tab_prevented(host: &web_sys::HtmlElement, shift: bool) -> bool {
    let init = web_sys::KeyboardEventInit::new();
    init.set_key("Tab");
    init.set_code("Tab");
    init.set_shift_key(shift);
    init.set_bubbles(true);
    init.set_cancelable(true);
    let event =
        web_sys::KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &init).unwrap();
    host.dispatch_event(&event).unwrap();
    event.default_prevented()
}

#[wasm_bindgen_test]
fn a_canvas_hands_tab_to_the_page_only_where_the_app_says_it_leaves() {
    let document = web_sys::window().unwrap().document().unwrap();
    let host: web_sys::HtmlElement = document.create_element("div").unwrap().dyn_into().unwrap();
    document.body().unwrap().append_child(&host).unwrap();
    let config = WebPlatformConfig {
        autofocus: false,
        ..WebPlatformConfig::default()
    };
    WebPlatform::with_host(host.clone(), config)
        .run(WindowConfig::default(), Idle)
        .expect("the platform mounted");
    let window = WINDOW.with(|kept| kept.borrow().clone()).expect("resumed");

    assert!(tab_prevented(&host, false), "the app walks its own order");
    window.set_tab_leaves(TabLeaves {
        forward: true,
        backward: false,
    });
    assert!(
        !tab_prevented(&host, false),
        "Tab leaves past the last stop"
    );
    assert!(tab_prevented(&host, true), "Shift+Tab still walks back");
}
