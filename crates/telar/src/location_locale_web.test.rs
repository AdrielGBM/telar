//! The locale as the first segment of a browser's address, kept in step with `set_locale` both ways, against a real session history. Needs a browser, so it runs under `wasm-bindgen-test` rather than `cargo test`.

#![cfg(all(target_arch = "wasm32", feature = "web-dom"))]

use std::cell::RefCell;
use std::sync::Arc;

use platform_core::{Event, LocationSource, WindowCommand};
use platform_web::WebLocation;
use telar::{
    Destination, Location, LocationFormat, address_of, current_locale, follow_location_locale,
    push_location, receive_location_history, set_locale, take_window_commands,
};
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

const BASE: &str = "/telar-locale-test";

thread_local! {
    static MOVES: RefCell<Vec<Vec<Location>>> = const { RefCell::new(Vec::new()) };
}

fn window() -> web_sys::Window {
    web_sys::window().unwrap()
}

fn path_now() -> String {
    let location = window().location();
    format!(
        "{}{}",
        location.pathname().unwrap(),
        location.hash().unwrap()
    )
}

fn entries() -> u32 {
    window().history().unwrap().length().unwrap()
}

async fn settle() {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        let _ = window().set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 100);
    });
    let _ = JsFuture::from(promise).await;
}

/// What the runner does with the moves the app queued: hands each to the platform's source.
fn carry_out(source: &mut WebLocation) {
    for command in take_window_commands() {
        if let WindowCommand::Navigate(update) = command {
            source.apply(&update);
        }
    }
}

/// What the runner does with a move the browser made by itself.
fn follow_the_browser(source: &mut WebLocation) {
    for history in MOVES.with(|moves| moves.take()) {
        receive_location_history(history);
    }
    carry_out(source);
}

#[wasm_bindgen_test]
async fn the_address_prefix_and_the_active_locale_follow_each_other() {
    platform_core::set_event_sink(Arc::new(|event| {
        if let Event::LocationChanged { history } = event {
            MOVES.with(|moves| moves.borrow_mut().push(history));
        }
    }));
    window()
        .history()
        .unwrap()
        .replace_state_with_url(
            &wasm_bindgen::JsValue::NULL,
            "",
            Some(&format!("{BASE}/es/doc#simulacion")),
        )
        .unwrap();

    follow_location_locale(["es", "en"], "es");
    let mut source = WebLocation::new(Some(LocationFormat::new(BASE)));
    let opened = source.initial();
    assert_eq!(
        opened,
        [Location::from_segments(["doc"])
            .with_locale("es")
            .with_fragment("simulacion")],
        "the first segment under the base is the locale"
    );
    receive_location_history(opened);
    carry_out(&mut source);
    assert_eq!(
        current_locale().as_deref(),
        Some("es"),
        "the address decides"
    );

    let before = entries();
    set_locale("en");
    carry_out(&mut source);
    assert_eq!(
        path_now(),
        format!("{BASE}/en/doc#simulacion"),
        "a switch keeps the page and the anchor"
    );
    assert_eq!(
        entries(),
        before,
        "and rewrites the entry rather than adding one"
    );
    assert_eq!(
        address_of(&Destination::Route(Location::from_segments(["projects"]))),
        format!("{BASE}/en/projects"),
        "a link is written in the locale the app is in"
    );

    platform_core::rename_anchor("simulacion", "simulation");
    carry_out(&mut source);
    assert_eq!(
        path_now(),
        format!("{BASE}/en/doc#simulation"),
        "an anchor whose name follows the locale takes the address with it"
    );

    assert!(push_location(Location::from_segments(["projects"])));
    carry_out(&mut source);
    assert_eq!(path_now(), format!("{BASE}/en/projects"));
    assert_eq!(entries(), before + 1);
    set_locale("es");
    carry_out(&mut source);
    assert_eq!(path_now(), format!("{BASE}/es/projects"));

    window().history().unwrap().back().unwrap();
    settle().await;
    follow_the_browser(&mut source);
    assert_eq!(
        current_locale().as_deref(),
        Some("es"),
        "back leaves the page, not the language"
    );
    assert_eq!(
        path_now(),
        format!("{BASE}/es/doc#simulation"),
        "the entry from before the switch is shown in the locale the app is in"
    );

    assert!(push_location(
        Location::from_segments(["doc"])
            .with_locale("en")
            .with_fragment("simulation")
    ));
    carry_out(&mut source);
    assert_eq!(current_locale().as_deref(), Some("en"));
    assert_eq!(
        path_now(),
        format!("{BASE}/en/doc#simulation"),
        "a link to the place shown in another language is a switch and nothing more"
    );
    assert_eq!(entries(), before + 1);
}
