//! The surface's primary scroll as the document's own scroll: the page grows with the content, the browser scrolls it, and the app hears where it went.
//!
//! Needs a browser, so it runs under `wasm-bindgen-test` rather than `cargo test` — see the crate README.

#![cfg(target_arch = "wasm32")]

use std::sync::{Arc, Mutex};

use geometry_core::Rect;
use platform_core::Event;
use platform_core::primary_scroll::DOCUMENT_SCROLL_ATTRIBUTE;
use renderer_core::{DrawCommand, Element, ElementId, RenderBackend, Role, Semantics};
use telar_renderer_dom::DomRenderer;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

const PAGE: u64 = 1;
const CONTENT: u64 = 2;
const OVERLAY: u64 = 3;
const CONTENT_HEIGHT: f32 = 4000.0;

static SCROLLED: Mutex<Vec<(u64, f32, f32)>> = Mutex::new(Vec::new());

fn listen_for_box_scrolls() {
    platform_core::set_event_sink(Arc::new(|event| {
        if let Event::BoxScrolled { box_id, x, y } = event {
            SCROLLED.lock().unwrap().push((box_id, x, y));
        }
    }));
    SCROLLED.lock().unwrap().clear();
}

fn scrolled() -> Vec<(u64, f32, f32)> {
    std::mem::take(&mut *SCROLLED.lock().unwrap())
}

fn window() -> web_sys::Window {
    web_sys::window().expect("a window")
}

fn document() -> web_sys::Document {
    window().document().expect("a document")
}

fn scroll_y() -> f64 {
    window().scroll_y().unwrap_or_default()
}

fn viewport_height() -> f32 {
    document()
        .document_element()
        .expect("a root element")
        .client_height() as f32
}

/// The host the built-in page gives the app: first in the body, the viewport's height, and a margin-less body around it.
fn host() -> web_sys::HtmlElement {
    let document = document();
    let body = document.body().expect("a body");
    let _ = body.style().set_property("margin", "0");
    let host: web_sys::HtmlElement = document
        .create_element("div")
        .expect("a host element")
        .dyn_into()
        .expect("a div is an HTML element");
    let _ = host.style().set_property("width", "100%");
    let _ = host.style().set_property("height", "100vh");
    let _ = host.style().set_property("touch-action", "none");
    body.insert_before(host.as_ref(), body.first_child().as_ref())
        .expect("the host went into the page");
    host
}

fn tear_down(host: web_sys::HtmlElement, renderer: DomRenderer) {
    drop(renderer);
    host.remove();
    window().scroll_to_with_x_and_y(0.0, 0.0);
}

fn element(id: u64, role: Role, layout: &str, rect: Rect) -> Element {
    Element::new(ElementId(id), Semantics::of(role), layout, rect)
}

/// A root page the viewport's size, holding content far taller than it — the frame a `ScrollPage` emits.
fn page(scroll_to: Option<(f32, f32)>, primary: bool) -> Vec<DrawCommand> {
    let viewport = Rect::new(0.0, 0.0, 800.0, viewport_height());
    let page = element(
        PAGE,
        Role::ScrollArea,
        "display:flex;flex-direction:column;",
        viewport,
    )
    .asking_to_scroll(scroll_to)
    .as_primary_scroll(primary);
    let content = element(
        CONTENT,
        Role::Group,
        &format!("height:{CONTENT_HEIGHT}px;"),
        Rect::new(0.0, 0.0, 800.0, CONTENT_HEIGHT),
    );
    vec![
        DrawCommand::PushElement {
            element: Arc::new(page),
        },
        DrawCommand::PushClip {
            rect: viewport,
            radius: Default::default(),
        },
        DrawCommand::PushElement {
            element: Arc::new(content),
        },
        DrawCommand::PopElement,
        DrawCommand::PopClip,
        DrawCommand::PopElement,
    ]
}

fn with_overlay(mut commands: Vec<DrawCommand>) -> Vec<DrawCommand> {
    let overlay = element(OVERLAY, Role::Group, "", Rect::new(10.0, 20.0, 100.0, 40.0));
    commands.push(DrawCommand::PushElement {
        element: Arc::new(overlay),
    });
    commands.push(DrawCommand::PopElement);
    commands
}

async fn next_frames() {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        let _ = window().set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 50);
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

fn node(host: &web_sys::HtmlElement, index: u32) -> web_sys::HtmlElement {
    host.child_nodes()
        .item(index)
        .expect("the frame placed the box")
        .dyn_into()
        .expect("a box is an HTML element")
}

#[wasm_bindgen_test]
fn the_document_scrolls_the_primary_scroll() {
    listen_for_box_scrolls();
    let host = host();
    let mut renderer = DomRenderer::new(host.clone()).expect("a renderer on the host");
    renderer.render_frame(&page(None, true), None).unwrap();

    assert!(host.has_attribute(DOCUMENT_SCROLL_ATTRIBUTE));
    assert_eq!(host.style().get_property_value("height").unwrap(), "auto");
    let root = document().document_element().unwrap();
    assert!(
        root.scroll_height() as f32 >= CONTENT_HEIGHT,
        "the page grew with the content: {}",
        root.scroll_height()
    );
    let style = window()
        .get_computed_style(node(&host, 0).as_ref())
        .unwrap()
        .unwrap();
    assert_eq!(
        style.get_property_value("overflow-y").unwrap(),
        "visible",
        "the page's box is not a scroller of its own"
    );
    window().scroll_to_with_x_and_y(0.0, 1000.0);
    assert_eq!(scroll_y(), 1000.0);
    tear_down(host, renderer);
}

#[wasm_bindgen_test]
fn content_wider_than_the_page_is_clipped_not_scrolled_sideways() {
    let host = host();
    let mut renderer = DomRenderer::new(host.clone()).expect("a renderer on the host");
    let mut commands = page(None, true);
    let wide = element(
        OVERLAY,
        Role::Group,
        "width:3000px;height:10px;flex-shrink:0;",
        Rect::new(0.0, 0.0, 3000.0, 10.0),
    );
    commands.insert(
        4,
        DrawCommand::PushElement {
            element: Arc::new(wide),
        },
    );
    commands.insert(5, DrawCommand::PopElement);
    renderer.render_frame(&commands, None).unwrap();

    let root = document().document_element().unwrap();
    assert_eq!(
        root.scroll_width(),
        root.client_width(),
        "the document gained a sideways scroll"
    );
    assert!(
        root.scroll_height() as f32 >= CONTENT_HEIGHT,
        "the page still scrolls down"
    );
    tear_down(host, renderer);
}

#[wasm_bindgen_test]
fn a_box_that_is_not_the_primary_scroll_scrolls_itself() {
    let host = host();
    let mut renderer = DomRenderer::new(host.clone()).expect("a renderer on the host");
    renderer.render_frame(&page(None, false), None).unwrap();
    assert!(!host.has_attribute(DOCUMENT_SCROLL_ATTRIBUTE));
    let style = window()
        .get_computed_style(node(&host, 0).as_ref())
        .unwrap()
        .unwrap();
    assert_eq!(style.get_property_value("overflow-y").unwrap(), "auto");
    tear_down(host, renderer);
}

#[wasm_bindgen_test]
async fn a_window_scroll_is_reported_as_the_page_scrolling() {
    listen_for_box_scrolls();
    let host = host();
    let mut renderer = DomRenderer::new(host.clone()).expect("a renderer on the host");
    renderer.render_frame(&page(None, true), None).unwrap();
    scrolled();

    window().scroll_to_with_x_and_y(0.0, 1200.0);
    next_frames().await;
    assert_eq!(scrolled().last(), Some(&(PAGE, 0.0, 1200.0)));
    tear_down(host, renderer);
}

#[wasm_bindgen_test]
fn a_request_to_move_the_page_scrolls_the_document() {
    let host = host();
    let mut renderer = DomRenderer::new(host.clone()).expect("a renderer on the host");
    renderer.render_frame(&page(None, true), None).unwrap();
    renderer
        .render_frame(&page(Some((0.0, 1500.0)), true), None)
        .unwrap();
    assert_eq!(scroll_y(), 1500.0);
    tear_down(host, renderer);
}

/// Before the app loads, the page is whatever the host already holds, and the reader may have scrolled it; the first frame keeps that position and tells the app about it.
#[wasm_bindgen_test]
fn a_scroll_made_before_the_app_loaded_is_kept() {
    listen_for_box_scrolls();
    let host = host();
    let _ = host.style().set_property("height", "auto");
    host.set_inner_html(&format!("<div style=\"height:{CONTENT_HEIGHT}px\"></div>"));
    window().scroll_to_with_x_and_y(0.0, 800.0);
    assert_eq!(scroll_y(), 800.0, "the page scrolled before the app loaded");

    let mut renderer = DomRenderer::new(host.clone()).expect("a renderer on the host");
    renderer.render_frame(&page(None, true), None).unwrap();
    assert_eq!(scroll_y(), 800.0);
    assert!(
        scrolled().contains(&(PAGE, 0.0, 800.0)),
        "the app starts where the reader already is"
    );
    tear_down(host, renderer);
}

/// The surface is the part of the page the viewport shows, whatever height the grown host has.
#[wasm_bindgen_test]
fn the_surface_is_measured_against_the_viewport() {
    let host = host();
    let mut renderer = DomRenderer::new(host.clone()).expect("a renderer on the host");
    renderer.render_frame(&page(None, true), None).unwrap();
    let surface = platform_web::WebWindow::new(host.clone());
    assert_eq!(surface.logical_size().1 as f32, viewport_height());

    window().scroll_to_with_x_and_y(0.0, 600.0);
    let (_, y) = surface.to_local(10.0, 50.0);
    assert_eq!(
        y, 50.0,
        "a point on the screen is where it is on the surface"
    );
    tear_down(host, renderer);
}

/// What the app places against the surface stands over the page as it scrolls.
#[wasm_bindgen_test]
fn a_box_placed_on_the_surface_stays_put_as_the_page_scrolls() {
    let host = host();
    let mut renderer = DomRenderer::new(host.clone()).expect("a renderer on the host");
    renderer
        .render_frame(&with_overlay(page(None, true)), None)
        .unwrap();
    window().scroll_to_with_x_and_y(0.0, 900.0);
    let overlay = node(&host, 1).get_bounding_client_rect();
    assert_eq!((overlay.x(), overlay.y()), (10.0, 20.0));
    tear_down(host, renderer);
}

#[wasm_bindgen_test]
fn a_page_that_stops_being_the_primary_scroll_gives_the_host_back() {
    let host = host();
    let mut renderer = DomRenderer::new(host.clone()).expect("a renderer on the host");
    renderer.render_frame(&page(None, true), None).unwrap();
    renderer.render_frame(&page(None, false), None).unwrap();
    assert!(!host.has_attribute(DOCUMENT_SCROLL_ATTRIBUTE));
    let style = host.style();
    assert_eq!(style.get_property_value("height").unwrap(), "100vh");
    assert_eq!(style.get_property_value("touch-action").unwrap(), "none");
    tear_down(host, renderer);
}
