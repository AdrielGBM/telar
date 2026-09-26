//! A box with a destination is a real `<a>`: the browser activates it, and the one plain click that stays in the app is handed back to it rather than reloading the page. Needs a browser, so it runs under `wasm-bindgen-test` rather than `cargo test`.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::sync::Arc;

use geometry_core::Rect;
use platform_core::{Destination, Event, EventHandler, Location, Platform, WindowConfig, external};
use platform_web::{WebPlatform, WebPlatformConfig, WebWindow};
use renderer_core::{DrawCommand, Element, ElementId, Focusable, RenderBackend, Role, Semantics};
use telar_renderer_dom::DomRenderer;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

const ROUTE: u64 = 201;
const WEB: u64 = 202;
const MAIL: u64 = 203;
const DISABLED: u64 = 204;
const INVALID_EXTERNAL: u64 = 205;

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

fn link(id: u64, destination: Destination, disabled: bool) -> Element {
    let semantics = Semantics::group()
        .linking_to(destination)
        .in_state(false, None, disabled)
        .focusable(Focusable {
            tab_stop: !disabled,
            consumes: Role::Link.consumed_keys(),
        });
    Element::new(
        ElementId(id),
        semantics,
        "width:100px;height:20px",
        Rect::new(0.0, 0.0, 100.0, 20.0),
    )
}

/// What a `to:` reads when the runtime value it built (`external(some_runtime_string)`) was rejected: still a link
/// role, but disabled and pointed nowhere, since [`IntoDestination`](platform_core::IntoDestination) flattened
/// its `None` rather than the box carrying a destination it does not have.
fn link_without_a_destination(id: u64) -> Element {
    let semantics = Semantics::of(Role::Link)
        .in_state(false, None, true)
        .focusable(Focusable {
            tab_stop: false,
            consumes: Role::Link.consumed_keys(),
        });
    Element::new(
        ElementId(id),
        semantics,
        "width:100px;height:20px",
        Rect::new(0.0, 0.0, 100.0, 20.0),
    )
}

fn render() {
    mount();
    let links = [
        link(
            ROUTE,
            Destination::Route(Location::root().segment("projects").segment("telar")),
            false,
        ),
        link(
            WEB,
            external("https://example.com/").expect("a scheme"),
            false,
        ),
        link(
            MAIL,
            external("mailto:someone@example.com").expect("a scheme"),
            false,
        ),
        link(
            DISABLED,
            Destination::Route(Location::root().segment("nowhere")),
            true,
        ),
        link_without_a_destination(INVALID_EXTERNAL),
    ];
    let mut commands = Vec::new();
    for element in links {
        commands.push(DrawCommand::PushElement {
            element: Arc::new(element),
        });
        commands.push(DrawCommand::PopElement);
    }
    SURFACE.with(|surface| {
        let mut surface = surface.borrow_mut();
        let surface = surface.as_mut().expect("mounted");
        surface
            .renderer
            .render_frame(&commands, None)
            .expect("the frame reconciled");
    });
}

fn host() -> web_sys::HtmlElement {
    SURFACE.with(|surface| surface.borrow().as_ref().expect("mounted").host.clone())
}

fn anchor(id: u64) -> web_sys::HtmlElement {
    host()
        .query_selector(&format!("[data-telar-focus=\"{id}\"]"))
        .expect("a valid selector")
        .unwrap_or_else(|| panic!("link {id} is in the document"))
        .dyn_into()
        .expect("an HTML element")
}

/// Clicks `target` and reports whether the app took the click back from the browser. A listener on the document, which hears the click after the app has, keeps the page from following it whatever the app decided.
fn click(target: &web_sys::HtmlElement, init: &web_sys::MouseEventInit) -> bool {
    init.set_bubbles(true);
    init.set_cancelable(true);
    let event =
        web_sys::MouseEvent::new_with_mouse_event_init_dict("click", init).expect("a click");
    let was_prevented = std::rc::Rc::new(std::cell::Cell::new(false));
    let seen = was_prevented.clone();
    let stop = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::Event)>::new(
        move |event: web_sys::Event| {
            seen.set(event.default_prevented());
            event.prevent_default();
        },
    );
    document()
        .add_event_listener_with_callback("click", stop.as_ref().unchecked_ref())
        .expect("listening");
    target.dispatch_event(&event).expect("dispatched");
    document()
        .remove_event_listener_with_callback("click", stop.as_ref().unchecked_ref())
        .expect("no longer listening");
    was_prevented.get()
}

fn pointer(kind: &str, target: &web_sys::HtmlElement, x: i32) {
    let init = web_sys::PointerEventInit::new();
    init.set_bubbles(true);
    init.set_cancelable(true);
    init.set_pointer_id(1);
    init.set_pointer_type("mouse");
    init.set_is_primary(true);
    init.set_buttons(if kind == "pointerup" { 0 } else { 1 });
    init.set_client_x(x);
    init.set_client_y(5);
    let event =
        web_sys::PointerEvent::new_with_event_init_dict(kind, &init).expect("a pointer event");
    target.dispatch_event(&event).expect("dispatched");
}

async fn next_frames() {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        let _ = web_sys::window()
            .expect("a window")
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 50);
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

fn activated() -> Vec<u64> {
    RECORDED.with(|recorded| {
        std::mem::take(&mut *recorded.borrow_mut())
            .into_iter()
            .filter_map(|event| match event {
                Event::BoxActivated { box_id } => Some(box_id),
                _ => None,
            })
            .collect()
    })
}

#[wasm_bindgen_test]
fn each_destination_is_an_anchor_with_the_address_a_browser_follows() {
    render();
    let route = anchor(ROUTE);
    assert_eq!(route.tag_name(), "A");
    assert_eq!(
        route.get_attribute("href").as_deref(),
        Some("/projects/telar")
    );
    assert_eq!(route.get_attribute("target"), None);
    assert_eq!(route.get_attribute("rel"), None);

    let web = anchor(WEB);
    assert_eq!(
        web.get_attribute("href").as_deref(),
        Some("https://example.com/")
    );
    assert_eq!(web.get_attribute("target").as_deref(), Some("_blank"));
    assert_eq!(web.get_attribute("rel").as_deref(), Some("noopener"));

    let mail = anchor(MAIL);
    assert_eq!(
        mail.get_attribute("target"),
        None,
        "a mail link opens its handler in place"
    );
    assert_eq!(mail.get_attribute("rel").as_deref(), Some("noopener"));

    assert_eq!(
        anchor(DISABLED).get_attribute("href"),
        None,
        "a disabled link goes nowhere"
    );
    assert_eq!(
        anchor(INVALID_EXTERNAL).get_attribute("href"),
        None,
        "a link whose runtime destination was rejected carries no href"
    );
}

#[wasm_bindgen_test]
async fn a_click_on_a_route_link_is_handed_back_to_the_app_without_the_capture_taking_it() {
    render();
    let route = anchor(ROUTE);
    activated();
    pointer("pointerdown", &route, 5);
    assert!(
        !host().has_pointer_capture(1),
        "a press alone leaves the pointer to what it pressed, so the browser's click still reaches the link"
    );
    pointer("pointerup", &route, 5);
    assert!(click(&route, &web_sys::MouseEventInit::new()));
    next_frames().await;
    assert_eq!(activated(), vec![ROUTE]);
}

#[wasm_bindgen_test]
async fn a_modified_click_an_external_one_and_a_disabled_one_are_the_browser_s() {
    render();
    activated();
    let ctrl = web_sys::MouseEventInit::new();
    ctrl.set_ctrl_key(true);
    assert!(
        !click(&anchor(ROUTE), &ctrl),
        "a new tab is the browser's to open"
    );
    let middle = web_sys::MouseEventInit::new();
    middle.set_button(1);
    assert!(!click(&anchor(ROUTE), &middle));
    assert!(!click(&anchor(WEB), &web_sys::MouseEventInit::new()));
    assert!(!click(&anchor(MAIL), &web_sys::MouseEventInit::new()));
    assert!(!click(&anchor(DISABLED), &web_sys::MouseEventInit::new()));
    assert!(!click(
        &anchor(INVALID_EXTERNAL),
        &web_sys::MouseEventInit::new()
    ));
    next_frames().await;
    assert_eq!(activated(), Vec::<u64>::new());
}
