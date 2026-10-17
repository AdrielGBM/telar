//! An input method composing through the element a document surface types into: where that element stands, and what the field hears of a composition. Synthetic composition events carry no input method behind them, so what is checked is what the entry makes of the sequence each browser reports. Needs a browser, so it runs under `wasm-bindgen-test` rather than `cargo test`.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::sync::Arc;

use geometry_core::Rect;
use platform_core::{Event, EventHandler, Ime, Key, Platform, WindowConfig};
use platform_web::{WebPlatform, WebPlatformConfig, WebWindow};
use renderer_core::{DrawCommand, Element, ElementId, Focusable, RenderBackend, Role, Semantics};
use telar_renderer_dom::DomRenderer;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

const FIELD: u64 = 201;
const OTHER: u64 = 202;
const CARET: Rect = Rect {
    x: 30.0,
    y: 2.0,
    width: 1.5,
    height: 16.0,
};

thread_local! {
    static RECORDED: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
    static SURFACE: RefCell<Option<Surface>> = const { RefCell::new(None) };
}

struct Surface {
    host: web_sys::HtmlElement,
    renderer: DomRenderer,
}

struct Recorder;

impl EventHandler<WebWindow> for Recorder {
    fn on_resume(&mut self, _window: &WebWindow) -> bool {
        true
    }

    fn on_event(&mut self, event: Event, _window: &WebWindow) {
        RECORDED.with(|recorded| recorded.borrow_mut().push(event));
    }

    fn on_redraw(&mut self, _window: &WebWindow) {}
}

fn document() -> web_sys::Document {
    web_sys::window()
        .and_then(|window| window.document())
        .expect("a document")
}

fn mount() {
    if SURFACE.with(|surface| surface.borrow().is_some()) {
        return;
    }
    let host: web_sys::HtmlElement = document()
        .create_element("div")
        .expect("a host element")
        .dyn_into()
        .expect("a div is an HTML element");
    document()
        .body()
        .expect("a body")
        .append_child(host.as_ref())
        .expect("the host went into the page");
    let config = WebPlatformConfig {
        host: None,
        autofocus: false,
        owns_gestures: true,
        owns_scroll: true,
        owns_context_menu: false,
        owns_keyboard: false,
        location: None,
    };
    WebPlatform::with_host(host.clone(), config)
        .run(WindowConfig::default(), Recorder)
        .expect("the platform mounted");
    let renderer = DomRenderer::new(host.clone()).expect("a renderer on the host");
    SURFACE.with(|surface| *surface.borrow_mut() = Some(Surface { host, renderer }));
}

fn host() -> web_sys::HtmlElement {
    SURFACE.with(|surface| surface.borrow().as_ref().expect("mounted").host.clone())
}

fn field(id: u64, focused: u64) -> Element {
    let semantics = Semantics::of(Role::TextInput)
        .in_state(id == focused, None, false)
        .focusable(Focusable {
            tab_stop: true,
            consumes: Role::TextInput.consumed_keys(),
        });
    Element::new(
        ElementId(id),
        semantics,
        "width:200px;height:20px",
        Rect::new(0.0, 0.0, 200.0, 20.0),
    )
    .with_caret((id == focused).then_some(CARET))
}

/// Two fields, the keyboard in `focused`.
fn render(focused: u64) {
    mount();
    let mut commands = Vec::new();
    for id in [FIELD, OTHER] {
        commands.push(DrawCommand::PushElement {
            element: Arc::new(field(id, focused)),
        });
        commands.push(DrawCommand::PopElement);
    }
    SURFACE.with(|surface| {
        let mut surface = surface.borrow_mut();
        surface
            .as_mut()
            .expect("mounted")
            .renderer
            .render_frame(&commands, None)
            .expect("the frame reconciled");
    });
}

fn entry() -> web_sys::HtmlTextAreaElement {
    host()
        .query_selector("textarea[data-telar-entry]")
        .expect("a valid selector")
        .expect("the entry is in the host")
        .dyn_into()
        .expect("the entry is a textarea")
}

fn compose(kind: &str, data: &str) {
    let init = web_sys::CompositionEventInit::new();
    init.set_data(data);
    init.set_bubbles(true);
    let event = web_sys::CompositionEvent::new_with_event_init_dict(kind, &init)
        .expect("a composition event");
    entry().dispatch_event(&event).expect("dispatched");
}

/// The `input` a browser fires once it has written `value` into the entry.
fn written(value: &str, composing: bool) {
    let entry = entry();
    entry.set_value(value);
    let init = web_sys::InputEventInit::new();
    init.set_is_composing(composing);
    init.set_input_type("insertCompositionText");
    init.set_data(Some(value));
    init.set_bubbles(true);
    let event =
        web_sys::InputEvent::new_with_event_init_dict("input", &init).expect("an input event");
    entry.dispatch_event(&event).expect("dispatched");
}

fn before_input(kind: &str, data: &str) {
    let init = web_sys::InputEventInit::new();
    init.set_input_type(kind);
    init.set_data(Some(data));
    init.set_bubbles(true);
    init.set_cancelable(true);
    let event = web_sys::InputEvent::new_with_event_init_dict("beforeinput", &init)
        .expect("an input event");
    entry().dispatch_event(&event).expect("dispatched");
}

async fn next_frames() {
    for _ in 0..2 {
        let promise = js_sys::Promise::new(&mut |resolve, _| {
            let _ = web_sys::window()
                .expect("a window")
                .request_animation_frame(&resolve);
        });
        let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
    }
}

fn recorded() -> Vec<Event> {
    RECORDED.with(|recorded| std::mem::take(&mut *recorded.borrow_mut()))
}

fn commits(events: &[Event]) -> Vec<String> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::Ime(Ime::Commit(text)) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn typed(events: &[Event]) -> String {
    events
        .iter()
        .filter_map(|event| match event {
            Event::KeyPressed {
                key: Key::Char(c), ..
            } => Some(*c),
            _ => None,
        })
        .collect()
}

#[wasm_bindgen_test]
fn the_entry_stands_on_the_caret_s_line() {
    render(FIELD);
    let host = host().get_bounding_client_rect();
    let at = entry().get_bounding_client_rect();
    assert_eq!((at.x() - host.x()).round(), CARET.x as f64);
    assert_eq!((at.y() - host.y()).round(), CARET.y as f64);
    assert_eq!(at.height().round(), CARET.height as f64);
    assert!(
        (at.x() + at.width() - host.x()).round() >= 200.0,
        "it runs to the field's far edge, so a composition has room along the line"
    );
}

/// **A composed word is typed once.** Each step reaches the field as the composition it is, its end as one commit, and the text a browser writes again once the composition is over — Safari's `insertFromComposition`, an `input` after the end — is no second insertion.
#[wasm_bindgen_test]
async fn a_composition_is_told_step_by_step_and_committed_once() {
    render(FIELD);
    next_frames().await;
    recorded();
    compose("compositionstart", "");
    compose("compositionupdate", "か");
    written("か", true);
    compose("compositionupdate", "かな");
    written("かな", true);
    compose("compositionend", "仮名");
    before_input("insertFromComposition", "仮名");
    written("仮名", false);
    next_frames().await;
    let events = recorded();
    assert!(
        events.contains(&Event::Ime(Ime::Preedit {
            text: "かな".into(),
            cursor: Some(6..6)
        })),
        "{events:?}"
    );
    assert_eq!(commits(&events), ["仮名"], "{events:?}");
    assert_eq!(typed(&events), "", "and nothing typed besides: {events:?}");
    assert_eq!(entry().value(), "", "the entry is empty for the next one");
}

/// Typing outside a composition is still typing.
#[wasm_bindgen_test]
async fn text_inserted_outside_a_composition_is_typed() {
    render(FIELD);
    next_frames().await;
    recorded();
    before_input("insertText", "a");
    next_frames().await;
    assert_eq!(typed(&recorded()), "a");
}

/// A press that takes the keyboard from the field ends the composition in the browser; the field types what it showed by itself, so the commit the browser reports for it would be the word a second time.
#[wasm_bindgen_test]
async fn a_press_that_ends_the_composition_commits_nothing_twice() {
    render(FIELD);
    next_frames().await;
    recorded();
    compose("compositionstart", "");
    compose("compositionupdate", "か");
    let init = web_sys::PointerEventInit::new();
    init.set_bubbles(true);
    let press = web_sys::PointerEvent::new_with_event_init_dict("pointerdown", &init)
        .expect("a pointer event");
    host().dispatch_event(&press).expect("dispatched");
    compose("compositionend", "か");
    next_frames().await;
    assert!(commits(&recorded()).is_empty());
}

/// The keyboard moving to another field mid-composition leaves the composition with the field it began in: nothing more of it reaches the next one.
#[wasm_bindgen_test]
async fn a_composition_stays_with_the_field_it_began_in() {
    render(FIELD);
    next_frames().await;
    recorded();
    compose("compositionstart", "");
    compose("compositionupdate", "か");
    render(OTHER);
    compose("compositionupdate", "かな");
    compose("compositionend", "仮名");
    next_frames().await;
    let events = recorded();
    assert!(commits(&events).is_empty(), "{events:?}");
    assert!(
        !events.contains(&Event::Ime(Ime::Preedit {
            text: "かな".into(),
            cursor: Some(6..6)
        })),
        "{events:?}"
    );
}

/// A key an input method is handling is its own even when the browser says the composition is over, as Safari does for the Enter that confirms one.
#[wasm_bindgen_test]
async fn a_key_the_input_method_is_handling_never_reaches_the_app() {
    render(FIELD);
    next_frames().await;
    recorded();
    let init = web_sys::KeyboardEventInit::new();
    init.set_key("Enter");
    init.set_key_code(229);
    init.set_bubbles(true);
    let event = web_sys::KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &init)
        .expect("a keyboard event");
    entry().dispatch_event(&event).expect("dispatched");
    next_frames().await;
    let events = recorded();
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, Event::KeyPressed { .. })),
        "{events:?}"
    );
}
