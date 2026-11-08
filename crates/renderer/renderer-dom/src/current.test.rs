//! The place under a bar fixed over a page the document scrolls, and the bar's link to it marked current: the browser scrolls, the page hears where it went, `use_anchor_at` answers with the section under the bar, and the link to that section is the one with `aria-current`.
//!
//! Needs a browser, so it runs under `wasm-bindgen-test` rather than `cargo test` — see the crate README.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use geometry_core::Size;
use layout_core::{LayoutStyle, SizeDimension};
use platform_core::{Event, anchor};
use reactive_core::{Memo, memo};
use renderer_core::{Color, RectStyle, RenderBackend};
use telar_renderer_dom::{CanvasTextMetrics, DomRenderer, ID_ATTRIBUTE};
use ui_core::{
    ComponentList, Container, FixedLayer, LayoutItem, NodeId, PageAnchor, ScrollPage,
    StyledContainer, box_item, use_anchor_at,
};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

const BAR_HEIGHT: f32 = 48.0;
const SECTIONS: [&str; 3] = ["one", "two", "three"];

thread_local! {
    static PAGE: RefCell<Option<Page>> = const { RefCell::new(None) };
}

static REPORTED: Mutex<Vec<Event>> = Mutex::new(Vec::new());

struct Page {
    tree: ComponentList,
    renderer: DomRenderer,
    section_height: f32,
    links: Vec<NodeId>,
    sections: Vec<NodeId>,
    under_bar: Memo<Option<Arc<str>>>,
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

/// A link in the bar to the section `name`, current while that section is under the bar.
fn link(name: &'static str, under_bar: Memo<Option<Arc<str>>>) -> StyledContainer {
    StyledContainer::new(
        LayoutStyle::new().width(80.0).height(32.0),
        |_| filled(10),
        vec![],
    )
    .expect("the link builds")
    .to(move || anchor(name))
    .current(move || under_bar.get().as_deref() == Some(name))
}

/// A bar fixed over the top of the surface with a link to each of three sections, each a screen and a half tall, on a page the document scrolls.
fn mount() {
    if PAGE.with(|page| page.borrow().is_some()) {
        return;
    }
    let root = document().document_element().expect("a root element");
    let (width, height) = (root.client_width() as f32, root.client_height() as f32);
    renderer_core::set_text_metrics(CanvasTextMetrics);
    ui_tree::set_element_capture(true);
    ui_core::set_surface_size(Size::new(width, height));
    platform_core::set_event_sink(Arc::new(|event| REPORTED.lock().unwrap().push(event)));

    let section_height = (height * 1.5).round();
    let under_bar = memo(|| use_anchor_at(BAR_HEIGHT));
    let links: Vec<StyledContainer> = SECTIONS
        .into_iter()
        .map(|name| link(name, under_bar))
        .collect();
    let link_nodes = links.iter().map(LayoutItem::layout_node).collect();
    let bar = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .gap(8.0)
            .width(SizeDimension::Percent(1.0))
            .height(BAR_HEIGHT)
            .padding_all(8.0),
        |_| filled(200),
        links
            .into_iter()
            .map(|link| box_item(link) as Box<dyn LayoutItem>)
            .collect(),
    )
    .expect("the bar builds");
    let sections: Vec<StyledContainer> = SECTIONS
        .into_iter()
        .enumerate()
        .map(|(index, name)| {
            StyledContainer::new(
                LayoutStyle::new()
                    .width(SizeDimension::Percent(1.0))
                    .height(section_height),
                move |_| filled(60 * index as u8),
                vec![],
            )
            .expect("a section builds")
            .page_anchor(move || name)
        })
        .collect();
    let section_nodes = sections.iter().map(LayoutItem::layout_node).collect();

    let mut page = ScrollPage::new_with(|_| {
        let layer = FixedLayer::new(LayoutStyle::new().flex_column(), vec![box_item(bar)])?;
        let mut children: Vec<Box<dyn LayoutItem>> = vec![box_item(layer)];
        children.extend(
            sections
                .into_iter()
                .map(|section| box_item(section) as Box<dyn LayoutItem>),
        );
        let content = Container::new(
            LayoutStyle::new()
                .flex_column()
                .width(SizeDimension::Percent(1.0)),
            children,
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
            section_height,
            links: link_nodes,
            sections: section_nodes,
            under_bar,
        })
    });
}

/// Hands the tree what the document reported since the last frame, as the platform does, and draws the next frame.
fn render() {
    mount();
    let reported = std::mem::take(&mut *REPORTED.lock().unwrap());
    PAGE.with(|page| {
        let mut page = page.borrow_mut();
        let page = page.as_mut().expect("mounted");
        for event in &reported {
            page.tree.on_event(event);
        }
        ui_core::relayout_if_dirty();
        let frame = page.tree.commands().clone();
        page.renderer
            .render_frame(&frame, None)
            .expect("the frame reconciled");
    });
}

fn with_page<R>(f: impl FnOnce(&Page) -> R) -> R {
    PAGE.with(|page| f(page.borrow().as_ref().expect("mounted")))
}

fn element_of(node: NodeId) -> web_sys::Element {
    let id: u64 = node.into();
    document()
        .query_selector(&format!("[{ID_ATTRIBUTE}=\"{id}\"]"))
        .ok()
        .flatten()
        .unwrap_or_else(|| panic!("box {id} is in the document"))
}

async fn next_frames() {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        let _ = window().set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 50);
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

/// Scrolls the document to `y`, lets it report the scroll and draws what follows from it.
async fn scroll_document_to(y: f32) {
    window().scroll_to_with_x_and_y(0.0, y as f64);
    next_frames().await;
    render();
    next_frames().await;
    render();
}

/// The `aria-current` of each link in the bar, in order.
fn currents() -> Vec<Option<String>> {
    with_page(|page| page.links.clone())
        .into_iter()
        .map(|link| element_of(link).get_attribute("aria-current"))
        .collect()
}

fn location() -> Option<String> {
    Some("location".to_string())
}

#[wasm_bindgen_test]
async fn the_link_to_the_section_under_the_bar_is_the_current_location() {
    render();
    scroll_document_to(0.0).await;
    assert_eq!(
        with_page(|page| page.under_bar.get()).as_deref(),
        Some("one")
    );
    assert_eq!(currents(), [location(), None, None]);

    let section_height = with_page(|page| page.section_height);
    scroll_document_to(section_height - BAR_HEIGHT + 2.0).await;
    let second = with_page(|page| page.sections[1]);
    let top = element_of(second).get_bounding_client_rect().top() as f32;
    assert!(
        top < BAR_HEIGHT,
        "the second section reached under the bar: its top is at {top}"
    );
    assert_eq!(
        with_page(|page| page.under_bar.get()).as_deref(),
        Some("two")
    );
    assert_eq!(currents(), [None, location(), None]);

    scroll_document_to(section_height - BAR_HEIGHT - 20.0).await;
    assert_eq!(
        currents(),
        [location(), None, None],
        "back above the line, the first section is under the bar again"
    );

    scroll_document_to(section_height * 2.0).await;
    assert_eq!(currents(), [None, None, location()]);
    window().scroll_to_with_x_and_y(0.0, 0.0);
}

#[wasm_bindgen_test]
async fn a_section_is_an_element_named_by_its_anchor() {
    render();
    let ids: Vec<Option<String>> = with_page(|page| page.sections.clone())
        .into_iter()
        .map(|section| element_of(section).get_attribute("id"))
        .collect();
    assert_eq!(
        ids,
        SECTIONS.map(|name| Some(name.to_string())),
        "the place the bar reads is the element a fragment reaches"
    );
}
