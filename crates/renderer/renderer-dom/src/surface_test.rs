//! A surface nested in the frame, as a document holds it: its boxes are elements of their own beside the window's that share their ids, and what the browser does to one of them reaches the app as a box event inside that surface. Needs a browser, so it runs under `wasm-bindgen-test` rather than `cargo test`.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::sync::Arc;

use geometry_core::Rect;
use platform_core::{Destination, Event, EventHandler, Location, Platform, WindowConfig};
use platform_web::{WebPlatform, WebPlatformConfig, WebWindow};
use renderer_core::{
    BorderRadius, Color, DrawCommand, Element, ElementId, Focusable, RectStyle, RenderBackend,
    Role, Semantics,
};
use telar_renderer_dom::{DomRenderer, prerender};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

const NESTED: u64 = 7;
const FRAME: u64 = 1;
const ROOT: u64 = 1;
const BUTTON: u64 = 2;
const LINK: u64 = 3;
const SCROLLER: u64 = 4;
const SCROLLED: u64 = 5;

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

fn boxed(id: u64, semantics: Semantics, layout: &str, rect: Rect) -> DrawCommand {
    DrawCommand::PushElement {
        element: Arc::new(Element::new(ElementId(id), semantics, layout, rect)),
    }
}

fn button(id: u64, rect: Rect) -> DrawCommand {
    let semantics = Semantics::of(Role::Button).focusable(Focusable {
        tab_stop: true,
        consumes: Role::Button.consumed_keys(),
    });
    boxed(id, semantics, "width:100px;height:20px", rect)
}

/// The window's box [`FRAME`] with surface [`NESTED`] drawn in it, whose boxes are numbered from the same start as the window's, and beside it the window's own button, which has the id of the surface's.
fn render() {
    mount();
    let link = Semantics::group()
        .linking_to(Destination::Route(Location::root().segment("inside")))
        .focusable(Focusable {
            tab_stop: true,
            consumes: Role::Link.consumed_keys(),
        });
    let scroller = Rect::new(0.0, 60.0, 100.0, 40.0);
    let commands = vec![
        boxed(
            FRAME,
            Semantics::group(),
            "width:300px;height:200px",
            Rect::new(0.0, 0.0, 300.0, 200.0),
        ),
        DrawCommand::PushSurface { surface: NESTED },
        boxed(
            ROOT,
            Semantics::group(),
            "display:flex;flex-direction:column;width:300px;height:200px",
            Rect::new(0.0, 0.0, 300.0, 200.0),
        ),
        button(BUTTON, Rect::new(0.0, 0.0, 100.0, 20.0)),
        DrawCommand::PopElement,
        boxed(
            LINK,
            link,
            "width:100px;height:20px",
            Rect::new(0.0, 20.0, 100.0, 20.0),
        ),
        DrawCommand::PopElement,
        boxed(
            SCROLLER,
            Semantics::of(Role::ScrollArea),
            "width:100px;height:40px;flex-shrink:0",
            scroller,
        ),
        DrawCommand::PushClip {
            rect: scroller,
            radius: BorderRadius::zero(),
        },
        boxed(
            SCROLLED,
            Semantics::group(),
            "width:100px;height:400px;flex-shrink:0",
            Rect::new(0.0, 60.0, 100.0, 400.0),
        ),
        DrawCommand::PopElement,
        DrawCommand::PopClip,
        DrawCommand::PopElement,
        DrawCommand::PopElement,
        DrawCommand::PopSurface,
        DrawCommand::PopElement,
        button(BUTTON, Rect::new(0.0, 250.0, 100.0, 20.0)),
        DrawCommand::PopElement,
    ];
    SURFACE.with(|surface| {
        let mut surface = surface.borrow_mut();
        let surface = surface.as_mut().expect("mounted");
        surface
            .renderer
            .render_frame(&commands, None)
            .expect("the frame reconciled");
    });
}

fn by(attribute: &str, value: &str) -> web_sys::HtmlElement {
    host()
        .query_selector(&format!("[{attribute}=\"{value}\"]"))
        .expect("a valid selector")
        .unwrap_or_else(|| panic!("an element whose {attribute} is {value}"))
        .dyn_into()
        .expect("an HTML element")
}

fn in_nested(event: Event) -> Event {
    Event::InSurface {
        surface: NESTED,
        event: Box::new(event),
    }
}

async fn next_frames() {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        let _ = web_sys::window()
            .expect("a window")
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 50);
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

fn recorded() -> Vec<Event> {
    RECORDED.with(|recorded| std::mem::take(&mut *recorded.borrow_mut()))
}

#[wasm_bindgen_test]
fn a_box_of_the_nested_surface_is_an_element_apart_from_the_windows_box_of_the_same_id() {
    render();
    let frame = by("data-telar-id", &FRAME.to_string());
    let root = by("data-telar-id", &format!("{NESTED}-{ROOT}"));
    assert!(
        !frame.is_same_node(Some(root.as_ref())),
        "the window's box and the surface's root are two elements"
    );
    assert!(
        root.parent_element()
            .is_some_and(|parent| parent.is_same_node(Some(frame.as_ref()))),
        "the surface's root is inside the box it is drawn in"
    );
    let inside = by("data-telar-id", &format!("{NESTED}-{BUTTON}"));
    let outside = by("data-telar-id", &BUTTON.to_string());
    assert!(!inside.is_same_node(Some(outside.as_ref())));
    let (placed, frame_rect) = (
        root.get_bounding_client_rect(),
        frame.get_bounding_client_rect(),
    );
    assert_eq!(
        (placed.x(), placed.y()),
        (frame_rect.x(), frame_rect.y()),
        "the surface's root is placed at its rect inside the box"
    );
}

#[wasm_bindgen_test]
async fn focus_on_a_box_of_the_nested_surface_reaches_the_app_inside_that_surface() {
    render();
    next_frames().await;
    recorded();
    by("data-telar-focus", &format!("{NESTED}-{BUTTON}"))
        .focus()
        .expect("the surface's button takes focus");
    next_frames().await;
    let events = recorded();
    assert!(
        events.contains(&in_nested(Event::BoxFocused { box_id: BUTTON })),
        "{events:?}"
    );
    assert!(
        !events.contains(&Event::BoxFocused { box_id: BUTTON }),
        "the window's box of the same id is not the one focused: {events:?}"
    );

    by("data-telar-focus", &BUTTON.to_string())
        .focus()
        .expect("the window's button takes focus");
    next_frames().await;
    let events = recorded();
    assert!(
        events.contains(&Event::BoxFocused { box_id: BUTTON }),
        "{events:?}"
    );
}

#[wasm_bindgen_test]
async fn a_link_of_the_nested_surface_is_followed_inside_that_surface() {
    render();
    next_frames().await;
    recorded();
    let init = web_sys::MouseEventInit::new();
    init.set_bubbles(true);
    init.set_cancelable(true);
    let click =
        web_sys::MouseEvent::new_with_mouse_event_init_dict("click", &init).expect("a click");
    by("data-telar-focus", &format!("{NESTED}-{LINK}"))
        .dispatch_event(&click)
        .expect("dispatched");
    assert!(click.default_prevented(), "the app follows the link itself");
    next_frames().await;
    let events = recorded();
    assert!(
        events.contains(&in_nested(Event::BoxActivated { box_id: LINK })),
        "{events:?}"
    );
}

#[wasm_bindgen_test]
async fn a_scroll_of_a_box_of_the_nested_surface_reaches_the_app_inside_that_surface() {
    render();
    next_frames().await;
    recorded();
    by("data-telar-id", &format!("{NESTED}-{SCROLLER}")).set_scroll_top(30);
    next_frames().await;
    let events = recorded();
    assert!(
        events.contains(&in_nested(Event::BoxScrolled {
            box_id: SCROLLER,
            x: 0.0,
            y: 30.0,
        })),
        "{events:?}"
    );
}

/// A page, a scrim the surface paints at its own level, and a dialog over both, as a modal open inside a frame draws them.
fn modal_in_frame(scrim: bool) -> Vec<DrawCommand> {
    let mut commands = vec![
        boxed(
            FRAME,
            Semantics::group(),
            "width:200px;height:100px",
            Rect::new(0.0, 0.0, 200.0, 100.0),
        ),
        DrawCommand::PushSurface { surface: NESTED },
        boxed(
            ROOT,
            Semantics::group(),
            "width:200px;height:100px",
            Rect::new(0.0, 0.0, 200.0, 100.0),
        ),
        DrawCommand::PopElement,
    ];
    if scrim {
        commands.push(DrawCommand::Rect {
            rect: Rect::new(0.0, 0.0, 200.0, 100.0),
            style: Arc::new(RectStyle::filled(Color::BLACK, 0.0)),
        });
    }
    commands.extend([
        boxed(
            BUTTON,
            Semantics::group(),
            "width:100px;height:60px",
            Rect::new(50.0, 20.0, 100.0, 60.0),
        ),
        DrawCommand::PopElement,
        DrawCommand::PopSurface,
        DrawCommand::PopElement,
    ]);
    commands
}

fn bare_host() -> web_sys::HtmlElement {
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
    host
}

/// What the box the surface is drawn in holds, in order: each box by the name it is written with, and paint as `paint`.
fn top_level(host: &web_sys::HtmlElement) -> (web_sys::Element, Vec<String>) {
    let frame = host
        .query_selector(&format!("[data-telar-id=\"{FRAME}\"]"))
        .expect("a valid selector")
        .expect("the frame's box is in the document");
    let children = frame.children();
    let order = (0..children.length())
        .filter_map(|index| children.item(index))
        .map(|child| {
            child
                .get_attribute("data-telar-id")
                .unwrap_or_else(|| "paint".to_string())
        })
        .collect();
    (frame, order)
}

fn background_of(element: &web_sys::Element) -> String {
    web_sys::window()
        .expect("a window")
        .get_computed_style(element)
        .expect("a computed style")
        .expect("styles for an element in the document")
        .get_property_value("background-color")
        .expect("a background colour")
}

#[wasm_bindgen_test]
fn a_scrim_a_frame_paints_at_its_own_level_stays_between_its_page_and_its_dialog() {
    let host = bare_host();
    let mut renderer = DomRenderer::new(host.clone()).expect("a renderer on the host");
    let render = |renderer: &mut DomRenderer, scrim| {
        renderer
            .render_frame(&modal_in_frame(scrim), None)
            .expect("the frame reconciled");
        top_level(&host)
    };
    let in_order = ["7-1", "paint", "7-2"].map(String::from).to_vec();

    let (frame, order) = render(&mut renderer, true);
    assert_eq!(order, in_order, "the dialog is drawn over the scrim");
    assert_eq!(
        background_of(&frame),
        "rgba(0, 0, 0, 0)",
        "the scrim is not the frame's background"
    );

    let (_, order) = render(&mut renderer, false);
    assert_eq!(order, ["7-1", "7-2"].map(String::from).to_vec());

    let (_, order) = render(&mut renderer, true);
    assert_eq!(order, in_order, "and comes back in its place");
    host.remove();
}

#[wasm_bindgen_test]
fn a_served_scrim_in_a_frame_is_taken_over_where_it_stands() {
    let host = bare_host();
    let written = prerender(&modal_in_frame(true), None);
    host.set_inner_html(&written.markup);
    let (_, served) = top_level(&host);
    assert_eq!(served, ["7-1", "paint", "7-2"].map(String::from).to_vec());
    let scrim = top_level(&host)
        .0
        .children()
        .item(1)
        .expect("the served scrim");

    let mut renderer = DomRenderer::new(host.clone()).expect("a renderer on the host");
    renderer
        .render_frame(&modal_in_frame(true), None)
        .expect("the frame reconciled");
    let (frame, order) = top_level(&host);
    assert_eq!(order, ["7-1", "paint", "7-2"].map(String::from).to_vec());
    assert!(
        scrim.is_same_node(frame.children().item(1).as_ref().map(AsRef::as_ref)),
        "the scrim the page was served with is the one kept"
    );
    host.remove();
}
