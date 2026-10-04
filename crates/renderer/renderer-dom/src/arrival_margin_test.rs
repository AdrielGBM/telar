//! The arrival margin of a page the document scrolls, under a 48px bar fixed over it: an anchor Telar follows, a fragment the browser follows and a control the browser scrolls to on focus all end clear of the bar instead of under it, and a page down leaves the bar out of its step.
//!
//! Needs a browser, so it runs under `wasm-bindgen-test` rather than `cargo test` — see the crate README.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use geometry_core::Size;
use layout_core::{LayoutStyle, SizeDimension};
use platform_core::Event;
use renderer_core::{Color, RectStyle, RenderBackend, Role};
use telar_renderer_dom::{CanvasTextMetrics, DomRenderer, ID_ATTRIBUTE};
use ui_core::{
    ComponentList, Container, FixedLayer, LayoutItem, NodeId, PageAnchor, ScrollPage,
    StyledContainer, box_item,
};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

const BAR_HEIGHT: f32 = 48.0;
const TARGET: &str = "arrival-target";

static POSTED: Mutex<Vec<Event>> = Mutex::new(Vec::new());

thread_local! {
    static PAGE: RefCell<Option<Page>> = const { RefCell::new(None) };
}

struct Page {
    tree: ComponentList,
    renderer: DomRenderer,
    target: NodeId,
    control: NodeId,
}

fn window() -> web_sys::Window {
    web_sys::window().expect("a window")
}

fn document() -> web_sys::Document {
    window().document().expect("a document")
}

fn filled(red: u8) -> RectStyle {
    RectStyle::filled(Color::from_rgb_u8(red, 90, 200), 0.0)
}

fn panel(height: f32, red: u8) -> StyledContainer {
    StyledContainer::new(
        LayoutStyle::new()
            .width(SizeDimension::Percent(1.0))
            .height(height),
        move |_| filled(red),
        vec![],
    )
    .expect("the panel builds")
}

/// The host the built-in page gives the app: first in the body, the viewport's size, and a margin-less body around it.
fn host() -> web_sys::HtmlElement {
    let body = document().body().expect("a body");
    let _ = body.style().set_property("margin", "0");
    let host: web_sys::HtmlElement = document()
        .create_element("div")
        .expect("a host element")
        .dyn_into()
        .expect("a div is an HTML element");
    let _ = host.style().set_property("width", "100%");
    let _ = host.style().set_property("height", "100vh");
    body.insert_before(host.as_ref(), body.first_child().as_ref())
        .expect("the host went into the page");
    host
}

/// A page holding, in order: a layer with a 48px bar across the top of the surface, two screens of page, a control, two more screens, the anchor, and two more.
fn mount() {
    if PAGE.with(|page| page.borrow().is_some()) {
        return;
    }
    let root = document().document_element().expect("a root element");
    let (width, height) = (root.client_width() as f32, root.client_height() as f32);
    renderer_core::set_text_metrics(CanvasTextMetrics);
    platform_core::set_event_sink(Arc::new(|event| {
        POSTED
            .lock()
            .expect("the queue is not poisoned")
            .push(event);
    }));
    ui_tree::set_element_capture(true);
    ui_core::set_surface_size(Size::new(width, height));

    let bar = panel(BAR_HEIGHT, 200);
    let control = StyledContainer::new(
        LayoutStyle::new().width(120.0).height(40.0),
        |_| filled(10),
        vec![],
    )
    .expect("the control builds")
    .on_press(|| ())
    .control(Role::Button);
    let control_node = control.layout_node();
    let target = panel(120.0, 60).page_anchor(|| TARGET);
    let target_node = target.layout_node();

    let mut page = ScrollPage::new_with(|_| {
        let layer = FixedLayer::new(LayoutStyle::new().flex_column(), vec![box_item(bar)])?;
        let content = Container::new(
            LayoutStyle::new()
                .flex_column()
                .width(SizeDimension::Percent(1.0)),
            vec![
                box_item(layer),
                box_item(panel(height * 2.0, 120)),
                box_item(control),
                box_item(panel(height * 2.0, 140)),
                box_item(target),
                box_item(panel(height * 2.0, 160)),
            ],
        )?;
        Ok(Box::new(content))
    })
    .expect("the page builds");
    page.relayout(width, height);
    let mut tree = ComponentList::new(page);
    tree.on_event(&Event::WindowResized {
        width: width as u32,
        height: height as u32,
    });
    let renderer = DomRenderer::new(host()).expect("a renderer on the host");
    PAGE.with(|slot| {
        *slot.borrow_mut() = Some(Page {
            tree,
            renderer,
            target: target_node,
            control: control_node,
        })
    });
}

fn render() {
    mount();
    ui_core::relayout_if_dirty();
    PAGE.with(|page| {
        let mut page = page.borrow_mut();
        let page = page.as_mut().expect("mounted");
        let frame = page.tree.commands().clone();
        page.renderer
            .render_frame(&frame, None)
            .expect("the frame reconciled");
    });
}

fn with_page<R>(f: impl FnOnce(&Page) -> R) -> R {
    PAGE.with(|page| f(page.borrow().as_ref().expect("mounted")))
}

fn element_of(node: NodeId) -> web_sys::HtmlElement {
    let id: u64 = node.into();
    document()
        .query_selector(&format!("[{ID_ATTRIBUTE}=\"{id}\"]"))
        .ok()
        .flatten()
        .unwrap_or_else(|| panic!("box {id} is in the document"))
        .dyn_into()
        .expect("a box is an HTML element")
}

fn top_of(node: NodeId) -> f64 {
    element_of(node).get_bounding_client_rect().top()
}

/// The app hears of the document's scroll as events, which a real runner would dispatch before the next frame.
fn deliver_events() {
    let events = std::mem::take(&mut *POSTED.lock().expect("the queue is not poisoned"));
    PAGE.with(|page| {
        let mut page = page.borrow_mut();
        let page = page.as_mut().expect("mounted");
        for event in events {
            page.tree.on_event(&event);
        }
    });
}

async fn next_frames() {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        let _ = window().set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 50);
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

async fn scroll_to(y: f64) {
    window().scroll_to_with_x_and_y(0.0, y);
    next_frames().await;
    deliver_events();
    render();
}

fn assert_just_below_the_bar(top: f64, what: &str) {
    assert!(
        (top - BAR_HEIGHT as f64).abs() < 1.0,
        "{what} starts just below the bar, at {top}"
    );
}

#[wasm_bindgen_test]
async fn the_document_scroller_is_padded_by_the_bar_fixed_over_the_page() {
    render();
    let style = document()
        .get_element_by_id("telar-arrival")
        .expect("the arrival margin's style is in the document");
    assert_eq!(
        style.text_content().as_deref(),
        Some(":root{scroll-padding:48px 0px 0px 0px}")
    );
    let root = document().document_element().expect("a root element");
    let padding = window()
        .get_computed_style(&root)
        .ok()
        .flatten()
        .and_then(|style| style.get_property_value("scroll-padding-top").ok());
    assert_eq!(padding.as_deref(), Some("48px"));
}

#[wasm_bindgen_test]
async fn an_anchor_telar_follows_lands_just_below_the_bar() {
    render();
    scroll_to(0.0).await;
    assert!(ui_core::follow(&platform_core::anchor(TARGET)));
    render();
    next_frames().await;
    let target = with_page(|page| page.target);
    assert_just_below_the_bar(top_of(target), "the anchor Telar revealed");
}

#[wasm_bindgen_test]
async fn a_fragment_the_browser_follows_lands_just_below_the_bar() {
    render();
    scroll_to(0.0).await;
    let _ = window().location().set_hash(TARGET);
    next_frames().await;
    let target = with_page(|page| page.target);
    assert_just_below_the_bar(top_of(target), "the fragment the browser followed");
    let _ = window().location().set_hash("");
}

/// Where the browser puts a control it scrolls to on focus is its own choice (Firefox centres one that was out of view), so this asks only that the control be in view and clear of the bar.
fn assert_clear_of_the_bar(control: NodeId, what: &str) {
    let rect = element_of(control).get_bounding_client_rect();
    let view = document()
        .document_element()
        .expect("a root element")
        .client_height() as f64;
    assert!(
        rect.top() >= BAR_HEIGHT as f64 - 1.0 && rect.bottom() <= view + 1.0,
        "{what} is in view and clear of the bar: {}..{} in a view {view} tall",
        rect.top(),
        rect.bottom()
    );
}

/// Where the control is in the document, whatever it is scrolled to.
fn document_top_of(node: NodeId) -> f64 {
    top_of(node) + window().scroll_y().unwrap_or_default()
}

#[wasm_bindgen_test]
async fn a_control_focused_from_out_of_view_is_scrolled_clear_of_the_bar() {
    render();
    let control = with_page(|page| page.control);
    let page_height = document()
        .document_element()
        .expect("a root element")
        .scroll_height() as f64;
    scroll_to(page_height).await;
    assert!(top_of(control) < 0.0, "the control is above the view");
    element_of(control)
        .focus()
        .expect("the control takes focus");
    next_frames().await;
    assert_clear_of_the_bar(control, "the control focused from below");
    let _ = element_of(control).blur();
}

#[wasm_bindgen_test]
async fn a_control_under_the_bar_is_scrolled_out_from_under_it_when_focused() {
    render();
    let control = with_page(|page| page.control);
    scroll_to(document_top_of(control) - 10.0).await;
    let top = top_of(control);
    assert!(
        top > 0.0 && top < BAR_HEIGHT as f64,
        "the control starts under the bar, at {top}"
    );
    element_of(control)
        .focus()
        .expect("the control takes focus");
    next_frames().await;
    assert_clear_of_the_bar(control, "the control focused under the bar");
    let _ = element_of(control).blur();
}

/// How far one page down moves the document from the top, through Firefox's `scrollByPages`, the step Page Down takes.
async fn one_page_down() -> f64 {
    scroll_to(0.0).await;
    let by_pages: js_sys::Function = js_sys::Reflect::get(&window(), &"scrollByPages".into())
        .expect("a window property")
        .dyn_into()
        .expect("Firefox pages a window with scrollByPages");
    by_pages
        .call1(&window(), &1.into())
        .expect("the page moves down");
    next_frames().await;
    window().scroll_y().unwrap_or_default()
}

#[wasm_bindgen_test]
async fn a_page_down_leaves_the_bar_out_of_its_step() {
    render();
    let padded = one_page_down().await;
    let style = document()
        .get_element_by_id("telar-arrival")
        .expect("the arrival margin's style is in the document");
    let rule = style.text_content();
    style.set_text_content(None);
    let bare = one_page_down().await;
    style.set_text_content(rule.as_deref());
    assert!(
        (bare - padded - BAR_HEIGHT as f64).abs() < 1.0,
        "a page down moves {padded} with the bar's margin and {bare} without it"
    );
}
