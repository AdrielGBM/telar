//! The session history as the app's address, against a real browser. Needs one, so it runs under `wasm-bindgen-test` rather than `cargo test`.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::sync::Arc;

use platform_core::{Event, Location, LocationFormat, LocationSource};
use telar_platform_web::WebLocation;
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

fn window() -> web_sys::Window {
    web_sys::window().unwrap()
}

fn path_now() -> String {
    let location = window().location();
    format!(
        "{}{}{}",
        location.pathname().unwrap(),
        location.search().unwrap(),
        location.hash().unwrap()
    )
}

async fn settle() {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        let _ = window().set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 100);
    });
    let _ = JsFuture::from(promise).await;
}

thread_local! {
    static MOVES: RefCell<Vec<Vec<Location>>> = const { RefCell::new(Vec::new()) };
}

fn record_moves() {
    platform_core::set_event_sink(Arc::new(|event| {
        if let Event::LocationChanged { history } = event {
            MOVES.with(|moves| moves.borrow_mut().push(history));
        }
    }));
}

fn at(path: &str) -> Location {
    LocationFormat::root().parse(path).unwrap()
}

#[wasm_bindgen_test]
async fn pushes_write_the_address_and_back_restores_the_stack_it_left() {
    record_moves();
    let mut source = WebLocation::new(Some(LocationFormat::new("/telar-test")));
    let opened = source.initial();
    assert!(
        opened.len() <= 1,
        "a page outside the base, or the page itself"
    );

    let first = vec![at("/"), at("/es")];
    source.replace(&first);
    assert_eq!(path_now(), "/telar-test/es");
    let second = vec![at("/"), at("/es"), at("/es/projects?tag=rust#telar")];
    source.push(&second);
    assert_eq!(path_now(), "/telar-test/es/projects?tag=rust#telar");

    window().history().unwrap().back().unwrap();
    settle().await;
    assert_eq!(path_now(), "/telar-test/es");
    let moves = MOVES.with(|moves| moves.take());
    assert_eq!(
        moves,
        [first],
        "the entry carries the history it was made from"
    );

    window().history().unwrap().forward().unwrap();
    settle().await;
    assert_eq!(MOVES.with(|moves| moves.take()), [second]);
}

#[wasm_bindgen_test]
async fn the_app_stepping_back_is_not_reported_back_to_it() {
    record_moves();
    let mut source = WebLocation::new(Some(LocationFormat::new("/telar-test")));
    source.initial();
    source.replace(&[at("/")]);
    source.push(&[at("/"), at("/a")]);
    source.back(1, &[at("/")]);
    settle().await;
    assert_eq!(path_now(), "/telar-test/");
    assert!(MOVES.with(|moves| moves.take()).is_empty());
}

#[wasm_bindgen_test]
async fn a_fragment_the_browser_opens_by_itself_is_a_new_entry_on_top() {
    record_moves();
    let mut source = WebLocation::new(Some(LocationFormat::new("/telar-test")));
    source.initial();
    source.replace(&[at("/"), at("/doc")]);
    window().location().set_hash("section").unwrap();
    settle().await;
    let moves = MOVES.with(|moves| moves.take());
    assert_eq!(
        moves,
        [vec![at("/"), at("/doc"), at("/doc#section")]],
        "reported once, though both popstate and hashchange fire"
    );
}

#[wasm_bindgen_test]
async fn an_address_closed_with_a_slash_by_a_static_host_opens_the_same_page_and_is_replaced_without_it()
 {
    window()
        .history()
        .unwrap()
        .replace_state_with_url(
            &wasm_bindgen::JsValue::NULL,
            "",
            Some("/telar-test/preview/card/?chrome=0"),
        )
        .unwrap();
    let mut source = WebLocation::new(Some(LocationFormat::new("/telar-test/")));
    let opened = source.initial();
    assert_eq!(opened, [at("/preview/card?chrome=0")]);
    source.replace(&opened);
    assert_eq!(path_now(), "/telar-test/preview/card?chrome=0");
}

async fn wait(ms: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        let _ = window().set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
    });
    let _ = JsFuture::from(promise).await;
}

/// A slider dragged across an arg the address carries replaces it on every frame, and a browser refuses a page that calls `replaceState` that often.
#[wasm_bindgen_test]
async fn a_burst_of_replaces_writes_the_first_at_once_and_the_last_once_it_rests() {
    let mut source = WebLocation::new(Some(LocationFormat::new("/telar-test")));
    source.initial();
    for n in 0..50 {
        source.replace(&[at(&format!("/drag/{n}"))]);
    }
    assert_eq!(path_now(), "/telar-test/drag/0");
    wait(400).await;
    assert_eq!(path_now(), "/telar-test/drag/49");
}

#[wasm_bindgen_test]
async fn a_replace_still_waiting_lands_on_its_entry_before_a_push() {
    record_moves();
    let mut source = WebLocation::new(Some(LocationFormat::new("/telar-test")));
    source.initial();
    source.replace(&[at("/a")]);
    source.replace(&[at("/b")]);
    source.push(&[at("/b"), at("/c")]);
    assert_eq!(path_now(), "/telar-test/c");
    window().history().unwrap().back().unwrap();
    settle().await;
    assert_eq!(path_now(), "/telar-test/b");
    assert_eq!(MOVES.with(|moves| moves.take()), [vec![at("/b")]]);
}

#[wasm_bindgen_test]
async fn a_replace_still_waiting_is_written_when_the_source_goes() {
    let mut source = WebLocation::new(Some(LocationFormat::new("/telar-test")));
    source.initial();
    source.replace(&[at("/first")]);
    source.replace(&[at("/last")]);
    drop(source);
    assert_eq!(path_now(), "/telar-test/last");
    wait(400).await;
    assert_eq!(path_now(), "/telar-test/last");
}

#[wasm_bindgen_test]
async fn a_replace_right_after_a_push_waits_out_the_interval_the_push_started() {
    let mut source = WebLocation::new(Some(LocationFormat::new("/telar-test")));
    source.initial();
    source.push(&[at("/"), at("/p")]);
    source.replace(&[at("/"), at("/p2")]);
    assert_eq!(path_now(), "/telar-test/p");
    wait(400).await;
    assert_eq!(path_now(), "/telar-test/p2");
}
