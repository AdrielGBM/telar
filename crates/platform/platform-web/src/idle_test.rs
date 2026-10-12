//! The host saying when the loop has no frame coming, against a real browser.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;

use platform_core::{Event, EventHandler, Platform, Window, WindowConfig};
use telar_platform_web::{IDLE_ATTRIBUTE, WebPlatform, WebPlatformConfig, WebWindow};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

thread_local! {
    static WINDOW: RefCell<Option<WebWindow>> = const { RefCell::new(None) };
}

/// Asks, after each turn, for the next wait in its queue, and for none once the queue is empty.
struct Waiting {
    waits: Rc<RefCell<VecDeque<Duration>>>,
}

impl EventHandler<WebWindow> for Waiting {
    fn on_resume(&mut self, window: &WebWindow) -> bool {
        WINDOW.with(|kept| *kept.borrow_mut() = Some(window.clone()));
        true
    }

    fn on_event(&mut self, _event: Event, _window: &WebWindow) {}

    fn on_redraw(&mut self, _window: &WebWindow) {}

    fn about_to_wait(&mut self) -> Option<Duration> {
        self.waits.borrow_mut().pop_front()
    }
}

async fn next_frame() {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        web_sys::window()
            .unwrap()
            .request_animation_frame(&resolve)
            .unwrap();
    });
    wasm_bindgen_futures::JsFuture::from(promise).await.unwrap();
}

async fn frames_until_idle(host: &web_sys::HtmlElement) -> usize {
    for frames in 1..=10 {
        next_frame().await;
        if host.has_attribute(IDLE_ATTRIBUTE) {
            return frames;
        }
    }
    panic!("the loop never went idle");
}

#[wasm_bindgen_test]
async fn the_host_is_idle_only_while_no_frame_is_coming() {
    let document = web_sys::window().unwrap().document().unwrap();
    let host: web_sys::HtmlElement = document.create_element("div").unwrap().dyn_into().unwrap();
    document.body().unwrap().append_child(&host).unwrap();
    let waits = Rc::new(RefCell::new(VecDeque::new()));
    let config = WebPlatformConfig {
        autofocus: false,
        ..WebPlatformConfig::default()
    };
    WebPlatform::with_host(host.clone(), config)
        .run(
            WindowConfig::default(),
            Waiting {
                waits: waits.clone(),
            },
        )
        .expect("the platform mounted");
    let window = WINDOW.with(|kept| kept.borrow().clone()).expect("resumed");

    assert!(
        !host.has_attribute(IDLE_ATTRIBUTE),
        "nothing is drawn before the first frame"
    );
    frames_until_idle(&host).await;

    waits.borrow_mut().extend([Duration::ZERO; 3]);
    window.request_redraw();
    assert!(
        !host.has_attribute(IDLE_ATTRIBUTE),
        "a requested frame is a frame coming"
    );
    frames_until_idle(&host).await;
    assert!(
        waits.borrow().is_empty(),
        "an app animating every frame kept the loop busy until it stopped"
    );

    waits.borrow_mut().push_back(Duration::from_secs(60));
    window.request_redraw();
    frames_until_idle(&host).await;
    assert!(
        waits.borrow().is_empty(),
        "a wait longer than a frame leaves the loop idle until it is due"
    );
}
