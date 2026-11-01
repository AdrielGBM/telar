//! The keyboard a document surface shares with its page: which keys the browser keeps, which the focused box keeps, and focus the browser moved on its own.
//!
//! A synthetic key event is untrusted, so the browser runs no default action for it — no Tab walk, no scroll. What is checked is the half that decides whether those defaults may run: the platform prevents a key exactly when the focused box declared it, and the boxes Tab may stop on are exactly the ones Telar says. Needs a browser, so it runs under `wasm-bindgen-test` rather than `cargo test`.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::sync::Arc;

use geometry_core::Rect;
use platform_core::{ConsumedKeys, Event, EventHandler, Key, NamedKey, Platform, WindowConfig};
use platform_web::{WebPlatform, WebPlatformConfig, WebWindow};
use renderer_core::{DrawCommand, Element, ElementId, Focusable, RenderBackend, Role, Semantics};
use telar_renderer_dom::DomRenderer;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

const BUTTON: u64 = 101;
const SLIDER: u64 = 102;
const MENU_ROW: u64 = 103;
const FIELD: u64 = 104;
const PLAIN: u64 = 105;
const TRAPPED: u64 = 106;
const PARAGRAPH: u64 = 107;

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

/// One surface for the whole page: the platform's listeners and loop are per page, as they are in an app.
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

fn control(id: u64, role: Role, tab_stop: bool, consumes: ConsumedKeys, focused: u64) -> Element {
    let semantics = Semantics::of(role)
        .in_state(id == focused, None, false)
        .focusable(Focusable { tab_stop, consumes });
    Element::new(
        ElementId(id),
        semantics,
        "width:100px;height:20px",
        Rect::new(0.0, 0.0, 100.0, 20.0),
    )
}

/// The frame a screen of controls would emit, with the keyboard on `focused` (`0` for nothing).
fn render(focused: u64) {
    mount();
    let plain = Element::new(
        ElementId(PLAIN),
        Semantics::group(),
        "width:100px;height:20px",
        Rect::new(0.0, 0.0, 100.0, 20.0),
    );
    let boxes = [
        control(
            BUTTON,
            Role::Button,
            true,
            Role::Button.consumed_keys(),
            focused,
        ),
        control(
            SLIDER,
            Role::Slider,
            true,
            Role::Slider.consumed_keys(),
            focused,
        ),
        control(
            MENU_ROW,
            Role::MenuItem,
            false,
            Role::MenuItem.consumed_keys(),
            focused,
        ),
        control(
            FIELD,
            Role::TextInput,
            true,
            Role::TextInput.consumed_keys(),
            focused,
        ),
        plain,
        control(
            TRAPPED,
            Role::Button,
            true,
            Role::Button.consumed_keys() | ConsumedKeys::TAB,
            focused,
        ),
    ];
    let mut commands = Vec::new();
    for element in boxes {
        commands.push(DrawCommand::PushElement {
            element: Arc::new(element),
        });
        commands.push(DrawCommand::PopElement);
    }
    commands.extend(paragraph());
    SURFACE.with(|surface| {
        let mut surface = surface.borrow_mut();
        let surface = surface.as_mut().expect("mounted");
        surface
            .renderer
            .render_frame(&commands, None)
            .expect("the frame reconciled");
    });
}

/// A paragraph with a link inside it, which the browser puts in its own Tab order and Telar does not.
fn paragraph() -> [DrawCommand; 3] {
    let spans: Arc<[renderer_core::Span]> = Arc::from(vec![
        renderer_core::Span::new(5..9, renderer_core::Declared::default()).linking_to(
            platform_core::Destination::Route(platform_core::Location::root().segment("docs")),
        ),
    ]);
    [
        DrawCommand::PushElement {
            element: Arc::new(Element::new(
                ElementId(PARAGRAPH),
                Semantics::group(),
                "width:300px;height:20px",
                Rect::new(0.0, 0.0, 300.0, 20.0),
            )),
        },
        DrawCommand::Text {
            text: Arc::from("Read docs first"),
            spans: Some(spans),
            rect: Rect::new(0.0, 0.0, 300.0, 20.0),
            style: Arc::new(renderer_core::TextStyle::new(
                14.0,
                renderer_core::Color::BLACK,
            )),
        },
        DrawCommand::PopElement,
    ]
}

fn link_run() -> web_sys::HtmlElement {
    host()
        .query_selector(&format!("[data-telar-run-box=\"{PARAGRAPH}\"]"))
        .expect("a valid selector")
        .expect("the link run is in the document")
        .dyn_into()
        .expect("an HTML element")
}

/// Focuses `element` as a keyboard would, so `:focus-visible` matches whatever the last input in the page was.
fn focus_visibly(element: &web_sys::HtmlElement) {
    let focus: js_sys::Function = js_sys::Reflect::get(element, &"focus".into())
        .expect("an element has focus()")
        .dyn_into()
        .expect("focus is a function");
    let options = js_sys::JSON::parse(r#"{"focusVisible":true}"#).expect("the options are JSON");
    focus
        .call1(element, &options)
        .expect("the element takes focus");
}

fn outline_of(element: &web_sys::Element) -> String {
    web_sys::window()
        .expect("a window")
        .get_computed_style(element)
        .expect("a computed style")
        .expect("styles for an element in the document")
        .get_property_value("outline-style")
        .expect("an outline style")
}

fn element(id: u64) -> web_sys::HtmlElement {
    host()
        .query_selector(&format!("[data-telar-focus=\"{id}\"]"))
        .expect("a valid selector")
        .unwrap_or_else(|| panic!("box {id} is focusable in the document"))
        .dyn_into()
        .expect("an HTML element")
}

fn active() -> Option<web_sys::Element> {
    document().active_element()
}

/// Presses `key` on `target` and reports whether its default action was prevented.
fn press(target: &web_sys::EventTarget, key: &str) -> bool {
    let init = web_sys::KeyboardEventInit::new();
    init.set_key(key);
    init.set_bubbles(true);
    init.set_cancelable(true);
    let event = web_sys::KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &init)
        .expect("a keyboard event");
    target.dispatch_event(&event).expect("dispatched");
    event.default_prevented()
}

fn press_on_focus(key: &str) -> bool {
    let target: web_sys::EventTarget = active().expect("something is focused").into();
    press(&target, key)
}

/// Waits out the platform's next turn. Its turn is an animation frame requested when the event arrived, so it runs before the second frame requested here, however late a loaded machine delivers frames; a fixed delay did not.
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

fn pressed(events: &[Event], wanted: NamedKey) -> bool {
    events.iter().any(
        |event| matches!(event, Event::KeyPressed { key: Key::Named(key), .. } if *key == wanted),
    )
}

#[wasm_bindgen_test]
fn tab_stops_are_the_boxes_telar_says_and_in_its_order() {
    render(0);
    let stops = host()
        .query_selector_all("[tabindex=\"0\"]")
        .expect("a valid selector");
    let ids: Vec<String> = (0..stops.length())
        .filter_map(|index| stops.item(index))
        .filter_map(|node| node.dyn_into::<web_sys::Element>().ok())
        .filter_map(|element| element.get_attribute("data-telar-focus"))
        .collect();
    assert_eq!(ids, ["101", "102", "104", "106"]);
    assert_eq!(
        element(MENU_ROW).get_attribute("tabindex").as_deref(),
        Some("-1"),
        "a row driven by arrows is focusable without being a stop"
    );
    let plain = host()
        .query_selector("[tabindex]:not([data-telar-focus]):not(textarea)")
        .expect("a valid selector");
    assert!(
        plain.is_none(),
        "a box that cannot hold focus is no stop either"
    );
}

#[wasm_bindgen_test]
fn a_slider_keeps_the_arrows_and_leaves_tab_and_paging_to_the_page() {
    render(SLIDER);
    let slider = element(SLIDER);
    assert_eq!(active().as_ref(), Some(slider.as_ref()));
    assert!(press_on_focus("ArrowDown"));
    assert!(press_on_focus("ArrowLeft"));
    assert!(!press_on_focus("Tab"), "Tab walks the page's own order");
    assert!(!press_on_focus("PageDown"), "the page scrolls");
    assert!(!press_on_focus(" "), "the page scrolls");
}

#[wasm_bindgen_test]
fn a_button_keeps_what_presses_it_and_leaves_the_arrows_to_scroll() {
    render(BUTTON);
    assert!(press_on_focus(" "));
    assert!(press_on_focus("Enter"));
    assert!(!press_on_focus("ArrowDown"));
    assert!(!press_on_focus("End"));
}

#[wasm_bindgen_test]
fn with_nothing_focused_every_key_is_the_page_s() {
    render(0);
    let host: web_sys::EventTarget = host().into();
    for key in ["ArrowDown", " ", "PageDown", "Home", "Tab"] {
        assert!(!press(&host, key), "{key}");
    }
}

#[wasm_bindgen_test]
fn a_chord_stays_the_browser_s_even_on_a_box_that_keeps_the_key() {
    render(SLIDER);
    let init = web_sys::KeyboardEventInit::new();
    init.set_key("ArrowDown");
    init.set_ctrl_key(true);
    init.set_bubbles(true);
    init.set_cancelable(true);
    let event = web_sys::KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &init)
        .expect("a keyboard event");
    element(SLIDER).dispatch_event(&event).expect("dispatched");
    assert!(!event.default_prevented());
}

#[wasm_bindgen_test]
fn inside_a_trap_tab_is_the_app_s() {
    render(TRAPPED);
    assert!(press_on_focus("Tab"));
}

#[wasm_bindgen_test]
async fn a_tab_left_to_the_browser_never_reaches_the_app() {
    render(BUTTON);
    next_frames().await;
    recorded();
    press_on_focus("Tab");
    press_on_focus("ArrowDown");
    next_frames().await;
    let events = recorded();
    assert!(
        !pressed(&events, NamedKey::Tab),
        "the browser walks focus, and the app hears where it landed instead"
    );
    assert!(
        pressed(&events, NamedKey::ArrowDown),
        "a key the page scrolls with still reaches the app's shortcuts"
    );
}

#[wasm_bindgen_test]
async fn a_tab_the_box_keeps_reaches_the_app() {
    render(TRAPPED);
    next_frames().await;
    recorded();
    press_on_focus("Tab");
    next_frames().await;
    assert!(pressed(&recorded(), NamedKey::Tab));
}

#[wasm_bindgen_test]
async fn a_box_the_browser_focuses_is_reported_as_focused() {
    render(0);
    next_frames().await;
    recorded();
    element(SLIDER).focus().expect("the slider takes focus");
    next_frames().await;
    let events = recorded();
    assert!(
        events.contains(&Event::BoxFocused { box_id: SLIDER }),
        "{events:?}"
    );
}

#[wasm_bindgen_test]
fn a_field_types_through_the_entry_and_keeps_its_caret_keys() {
    render(FIELD);
    let entry = active().expect("the entry holds the keyboard");
    assert_eq!(entry.tag_name().to_ascii_lowercase(), "textarea");
    assert!(press_on_focus("ArrowLeft"));
    assert!(press_on_focus("Home"));
    assert!(press_on_focus("Backspace"));
}

#[wasm_bindgen_test]
fn tab_in_a_field_starts_the_browser_s_walk_from_the_field() {
    render(FIELD);
    let entry: web_sys::EventTarget = active().expect("the entry holds the keyboard").into();
    let prevented = press(&entry, "Tab");
    assert!(!prevented);
    assert_eq!(
        active().as_ref(),
        Some(element(FIELD).as_ref()),
        "the walk starts where the caret is, not at the end of the app"
    );
}

fn beside_the_app() -> web_sys::HtmlElement {
    let existing = document().get_element_by_id("beside-the-app");
    let element = existing.unwrap_or_else(|| {
        let link = document().create_element("button").expect("a button");
        link.set_id("beside-the-app");
        document()
            .body()
            .expect("a body")
            .append_child(link.as_ref())
            .expect("the button went into the page");
        link
    });
    element.dyn_into().expect("an HTML element")
}

#[wasm_bindgen_test]
async fn focus_moved_beside_the_app_leaves_no_box_holding_it() {
    render(SLIDER);
    next_frames().await;
    recorded();
    beside_the_app()
        .focus()
        .expect("the page's button takes focus");
    next_frames().await;
    let events = recorded();
    beside_the_app()
        .blur()
        .expect("the page's button lets focus go");
    assert!(events.contains(&Event::FocusLeftBoxes), "{events:?}");
}

#[wasm_bindgen_test]
async fn focus_moved_between_boxes_stays_the_app_s() {
    render(SLIDER);
    next_frames().await;
    recorded();
    element(BUTTON).focus().expect("the button takes focus");
    next_frames().await;
    let events = recorded();
    assert!(!events.contains(&Event::FocusLeftBoxes), "{events:?}");
    assert!(events.contains(&Event::BoxFocused { box_id: BUTTON }));
}

#[wasm_bindgen_test]
async fn focus_on_a_link_inside_a_paragraph_leaves_no_box_holding_it() {
    render(BUTTON);
    next_frames().await;
    recorded();
    focus_visibly(&link_run());
    next_frames().await;
    let events = recorded();
    assert!(
        events.contains(&Event::FocusLeftBoxes),
        "the button would keep its ring and its Enter: {events:?}"
    );
}

#[wasm_bindgen_test]
async fn the_entry_taking_the_keyboard_is_no_move_away_from_the_field() {
    render(0);
    next_frames().await;
    recorded();
    render(FIELD);
    next_frames().await;
    let entry = active().expect("the entry holds the keyboard");
    assert_eq!(entry.tag_name().to_ascii_lowercase(), "textarea");
    let events = recorded();
    assert!(!events.contains(&Event::FocusLeftBoxes), "{events:?}");
}

#[wasm_bindgen_test]
async fn what_telar_draws_no_ring_for_keeps_the_browser_s() {
    render(0);
    next_frames().await;
    let link = link_run();
    focus_visibly(&link);
    assert_ne!(
        outline_of(&link),
        "none",
        "nothing else would show the keyboard is on the link"
    );
    let button = element(BUTTON);
    focus_visibly(&button);
    assert_eq!(
        outline_of(&button),
        "none",
        "a box wears Telar's ring, and only that one"
    );
    next_frames().await;
    recorded();
}

/// Presses the mouse down on `target` and reports whether its default action — focusing what it lands on — was refused.
fn mouse_down(target: &web_sys::HtmlElement) -> bool {
    let init = web_sys::MouseEventInit::new();
    init.set_bubbles(true);
    init.set_cancelable(true);
    let event = web_sys::MouseEvent::new_with_mouse_event_init_dict("mousedown", &init)
        .expect("a mouse event");
    target.dispatch_event(&event).expect("dispatched");
    event.default_prevented()
}

/// A press on a box that is no stop — a tree's row, a toolbar's item — leaves the document's focus with the control that drives it, so the keys after it are read against that control and not the row. A synthetic press focuses nothing itself; what is checked is that the default which would focus the row is refused, and the one that would focus a stop is not.
#[wasm_bindgen_test]
fn a_press_on_a_row_leaves_the_keyboard_with_the_control_that_holds_it() {
    render(BUTTON);
    assert!(
        mouse_down(&element(MENU_ROW)),
        "the row does not take the keyboard from the control"
    );
    assert!(
        !mouse_down(&element(SLIDER)),
        "a stop is focused by a press"
    );
    render(MENU_ROW);
    assert!(
        !mouse_down(&element(MENU_ROW)),
        "a row Telar gave focus to keeps it"
    );
    render(0);
}

/// A key a script dispatches has no physical key behind it, and neither does one from an on-screen keyboard or an input method: each is read as the key it is, never as a repeat of the last one that came without a key up.
#[wasm_bindgen_test]
async fn a_key_with_no_physical_key_is_read_as_itself() {
    render(BUTTON);
    next_frames().await;
    recorded();
    press_on_focus("ArrowDown");
    assert!(press_on_focus(" "), "the button keeps the Space it is");
    next_frames().await;
    let events = recorded();
    assert!(pressed(&events, NamedKey::ArrowDown), "{events:?}");
    assert!(pressed(&events, NamedKey::Space), "{events:?}");
}
