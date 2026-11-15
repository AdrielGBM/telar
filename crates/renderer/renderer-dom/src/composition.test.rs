//! An input method composing through the element a document surface types into: where that element stands, and what the field hears of a composition. Synthetic composition events carry no input method behind them, so what is checked is what the entry makes of the sequence each browser reports. Needs a browser, so it runs under `wasm-bindgen-test` rather than `cargo test`.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::sync::Arc;

use geometry_core::Rect;
use platform_core::{Event, EventHandler, Ime, Key, Platform, TextSelection, WindowConfig};
use platform_web::{PageNames, WebPlatform, WebPlatformConfig, WebWindow};
use renderer_core::{
    DrawCommand, Editing, Element, ElementId, Focusable, RenderBackend, Role, Semantics,
};
use telar_renderer_dom::DomRenderer;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

const FIELD: u64 = 201;
const OTHER: u64 = 202;
const LINES: u64 = 203;
const CARET: Rect = Rect {
    x: 30.0,
    y: 2.0,
    width: 1.5,
    height: 16.0,
};
/// What the focused field shows, its caret after the first letter.
const SHOWN: &str = "ab";

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
        page: PageNames::default(),
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
    let role = match id {
        LINES => Role::MultilineTextInput,
        _ => Role::TextInput,
    };
    let semantics = Semantics::of(role)
        .in_state(id == focused, None, false)
        .focusable(Focusable {
            tab_stop: true,
            consumes: role.consumed_keys(),
        });
    Element::new(
        ElementId(id),
        semantics,
        "width:200px;height:20px",
        Rect::new(0.0, 0.0, 200.0, 20.0),
    )
    .editing((id == focused).then(|| Editing::new(SHOWN, 1, 1, CARET, false)))
}

/// Two fields of one line and one of several, the keyboard in `focused`.
fn render(focused: u64) {
    mount();
    let mut commands = Vec::new();
    for id in [FIELD, OTHER, LINES] {
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

fn entry() -> web_sys::HtmlElement {
    host()
        .query_selector("[data-telar-entry]")
        .expect("a valid selector")
        .expect("the entry is in the host")
        .dyn_into()
        .expect("the entry is an HTML element")
}

fn line() -> web_sys::HtmlInputElement {
    entry()
        .dyn_into()
        .expect("the entry of a field of one line is an input")
}

fn compose(kind: &str, data: &str) {
    let init = web_sys::CompositionEventInit::new();
    init.set_data(data);
    init.set_bubbles(true);
    let event = web_sys::CompositionEvent::new_with_event_init_dict(kind, &init)
        .expect("a composition event");
    entry().dispatch_event(&event).expect("dispatched");
}

/// The `input` a browser fires once it has written `value` into the entry, its caret `caret` UTF-16 units in.
fn written(value: &str, caret: u32, composing: bool) {
    let entry = line();
    entry.set_value(value);
    entry
        .set_selection_range(caret, caret)
        .expect("the entry takes a selection");
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

/// **A composed word is typed once.** Composed at the caret of a field that already shows text, each step reaches the field as the composition it is, without the text around it, its end as one commit, and the text a browser writes again once the composition is over — Safari's `insertFromComposition`, an `input` after the end — is no second insertion.
#[wasm_bindgen_test]
async fn a_composition_is_told_step_by_step_and_committed_once() {
    render(FIELD);
    next_frames().await;
    recorded();
    compose("compositionstart", "");
    compose("compositionupdate", "か");
    written("aかb", 2, true);
    compose("compositionupdate", "かな");
    written("aかなb", 3, true);
    compose("compositionend", "仮名");
    before_input("insertFromComposition", "仮名");
    written("a仮名b", 3, false);
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
    assert_eq!(
        line().value(),
        "a仮名b",
        "the entry shows the field with the word in place of its caret"
    );
    assert_eq!(
        (line().selection_start(), line().selection_end()),
        (Ok(Some(3)), Ok(Some(3)))
    );
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

/// What an input method writes straight into the entry, with no `beforeinput` ahead of it, is typed as what it put at the caret.
#[wasm_bindgen_test]
async fn text_an_input_method_wrote_directly_is_typed() {
    render(FIELD);
    next_frames().await;
    recorded();
    written("aXYb", 3, false);
    next_frames().await;
    assert_eq!(typed(&recorded()), "XY");
}

/// A change the browser made itself that is no insertion at the caret is typed as nothing, and the entry goes back to showing the field.
#[wasm_bindgen_test]
async fn a_change_that_is_no_insertion_at_the_caret_is_undone() {
    render(FIELD);
    next_frames().await;
    recorded();
    written("zz", 2, false);
    next_frames().await;
    assert_eq!(typed(&recorded()), "");
    assert_eq!(line().value(), SHOWN);
    assert_eq!(
        (line().selection_start(), line().selection_end()),
        (Ok(Some(1)), Ok(Some(1)))
    );
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

/// The keyboard moving to a field of several lines moves the entry to a `textarea`, and back to an `input` for one of a single line: one entry at a time, holding the keyboard and still typing.
#[wasm_bindgen_test]
async fn the_entry_becomes_the_kind_of_field_the_keyboard_is_in() {
    render(FIELD);
    render(LINES);
    next_frames().await;
    recorded();
    let entries = host()
        .query_selector_all("[data-telar-entry]")
        .expect("a valid selector");
    assert_eq!(entries.length(), 1);
    let active = document().active_element().expect("something is focused");
    assert_eq!(active.tag_name().to_ascii_lowercase(), "textarea");
    assert!(active.has_attribute("data-telar-entry"));
    before_input("insertText", "a");
    next_frames().await;
    assert_eq!(typed(&recorded()), "a");

    render(FIELD);
    let active = document().active_element().expect("something is focused");
    assert_eq!(active.tag_name().to_ascii_lowercase(), "input");
    assert_eq!(line().value(), SHOWN);
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

fn selection_moved(start: u32, end: u32, direction: &str) {
    line()
        .set_selection_range_with_direction(start, end, direction)
        .expect("the entry takes a selection");
    let event = web_sys::Event::new("selectionchange").expect("an event");
    entry().dispatch_event(&event).expect("dispatched");
}

fn selections(events: &[Event]) -> Vec<TextSelection> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::SelectionMoved { selection } => Some(*selection),
            _ => None,
        })
        .collect()
}

/// A selection a screen reader or a selection handle moves in the entry reaches the field, with the end the caret is at kept; the field's own selection, written into the entry, reaches it as nothing.
#[wasm_bindgen_test]
async fn a_selection_moved_in_the_entry_reaches_the_field() {
    render(FIELD);
    next_frames().await;
    recorded();
    selection_moved(1, 1, "none");
    next_frames().await;
    assert_eq!(selections(&recorded()), [], "the field's own caret");

    selection_moved(0, 2, "backward");
    next_frames().await;
    let moved = selections(&recorded());
    assert!(!moved.is_empty());
    assert!(
        moved.iter().all(|selection| *selection
            == TextSelection {
                anchor: 2,
                focus: 0
            }),
        "{moved:?}"
    );
}
