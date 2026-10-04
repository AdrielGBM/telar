//! A layer fixed over the surface, on a page the document scrolls: it stays put over the sticky stage the page scrolls under it, the browser hands it the pointer only over its own boxes, and its controls sit in the document where the layer was declared, which is the order Tab walks.
//!
//! Needs a browser, so it runs under `wasm-bindgen-test` rather than `cargo test` — see the crate README.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;

use geometry_core::Size;
use layout_core::{LayoutStyle, SizeDimension};
use platform_core::Event;
use renderer_core::{Color, RectStyle, RenderBackend, Role};
use telar_renderer_dom::{CanvasTextMetrics, DomRenderer, ID_ATTRIBUTE};
use ui_core::{
    ComponentList, Container, FixedLayer, LayoutItem, NodeId, ScrollPage, StyledContainer, box_item,
};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

const BAR_HEIGHT: f32 = 48.0;

thread_local! {
    static PAGE: RefCell<Option<Page>> = const { RefCell::new(None) };
}

struct Page {
    tree: ComponentList,
    renderer: DomRenderer,
    width: f32,
    height: f32,
    page: NodeId,
    bar: NodeId,
    bar_button: NodeId,
    stage: NodeId,
    before: NodeId,
    after: NodeId,
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

fn button() -> StyledContainer {
    StyledContainer::new(
        LayoutStyle::new().width(80.0).height(32.0),
        |_| filled(10),
        vec![],
    )
    .expect("the button builds")
    .on_press(|| ())
    .control(Role::Button)
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

/// A page holding, in order: a button, a layer with a bar across the top of the surface, a track three screens tall whose stage sticks to the top, a second button, and more page.
fn mount() {
    if PAGE.with(|page| page.borrow().is_some()) {
        return;
    }
    let root = document().document_element().expect("a root element");
    let (width, height) = (root.client_width() as f32, root.client_height() as f32);
    renderer_core::set_text_metrics(CanvasTextMetrics);
    ui_tree::set_element_capture(true);
    ui_core::set_surface_size(Size::new(width, height));

    let before = button();
    let before_node = before.layout_node();
    let bar_button = button();
    let bar_button_node = bar_button.layout_node();
    let bar = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .width(SizeDimension::Percent(1.0))
            .height(BAR_HEIGHT)
            .padding_all(8.0),
        |_| filled(200),
        vec![box_item(bar_button)],
    )
    .expect("the bar builds");
    let bar_node = bar.layout_node();
    let stage = StyledContainer::new(
        LayoutStyle::new().sticky().inset_top(0.0).height(height),
        |_| filled(120),
        vec![],
    )
    .expect("the stage builds");
    let stage_node = stage.layout_node();
    let after = button();
    let after_node = after.layout_node();

    let content_node = std::cell::Cell::new(None);
    let mut page = ScrollPage::new_with(|_| {
        let layer = FixedLayer::new(LayoutStyle::new().flex_column(), vec![box_item(bar)])?;
        let track = Container::new(
            LayoutStyle::new().flex_column().height(height * 3.0),
            vec![box_item(stage)],
        )?;
        let content = Container::new(
            LayoutStyle::new()
                .flex_column()
                .width(SizeDimension::Percent(1.0)),
            vec![
                box_item(before),
                box_item(layer),
                box_item(track),
                box_item(after),
                box_item(Container::new(LayoutStyle::new().height(height * 2.0), vec![]).unwrap()),
            ],
        )?;
        content_node.set(Some(content.layout_node()));
        Ok(Box::new(content))
    })
    .expect("the page builds");
    page.relayout(width, height);
    let page_node = content_node.get().expect("the page built its content");
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
            width,
            height,
            page: page_node,
            bar: bar_node,
            bar_button: bar_button_node,
            stage: stage_node,
            before: before_node,
            after: after_node,
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

fn element_of(node: NodeId) -> web_sys::Element {
    let id: u64 = node.into();
    document()
        .query_selector(&format!("[{ID_ATTRIBUTE}=\"{id}\"]"))
        .ok()
        .flatten()
        .unwrap_or_else(|| panic!("box {id} is in the document"))
}

fn id_of(element: &web_sys::Element) -> Option<u64> {
    element.get_attribute(ID_ATTRIBUTE)?.parse().ok()
}

/// The box the browser hands a pointer at `(x, y)` to: the nearest element up from the one it hit that stands for a box.
fn box_at(x: f32, y: f32) -> Option<u64> {
    let mut at = document().element_from_point(x, y);
    while let Some(element) = at {
        if let Some(id) = id_of(&element) {
            return Some(id);
        }
        at = element.parent_element();
    }
    None
}

async fn next_frames() {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        let _ = window().set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 50);
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

#[wasm_bindgen_test]
async fn the_layer_stays_over_the_sticky_stage_the_document_scrolls_under_it() {
    render();
    let (width, height, bar, bar_button, stage) = with_page(|page| {
        (
            page.width,
            page.height,
            page.bar,
            page.bar_button,
            page.stage,
        )
    });
    window().scroll_to_with_x_and_y(0.0, (height * 1.5) as f64);
    next_frames().await;
    render();
    next_frames().await;

    assert!(
        window().scroll_y().unwrap_or_default() > 0.0,
        "the document scrolled"
    );
    let stage_top = element_of(stage).get_bounding_client_rect().top();
    assert!(
        stage_top.abs() < 1.0,
        "the stage is stuck at the top: {stage_top}"
    );
    let bar_rect = element_of(bar).get_bounding_client_rect();
    assert!(
        bar_rect.top().abs() < 1.0,
        "the bar stayed at the top: {}",
        bar_rect.top()
    );
    assert!(((bar_rect.height() as f32) - BAR_HEIGHT).abs() < 1.0);

    let bar_id: u64 = bar.into();
    let bar_button_id: u64 = bar_button.into();
    let stage_id: u64 = stage.into();
    let on_bar = box_at(width - 20.0, BAR_HEIGHT / 2.0);
    assert_eq!(
        on_bar,
        Some(bar_id),
        "the bar is drawn over the stage stuck under it"
    );
    assert_eq!(box_at(48.0, BAR_HEIGHT / 2.0), Some(bar_button_id));
    assert_eq!(
        box_at(width / 2.0, height / 2.0),
        Some(stage_id),
        "the layer spans the surface, but the pointer reaches the page wherever the layer has no box"
    );
}

#[wasm_bindgen_test]
async fn the_layer_sits_in_the_document_where_it_was_declared() {
    render();
    let (page, bar, before, bar_button, after) = with_page(|page| {
        (
            page.page,
            page.bar,
            page.before,
            page.bar_button,
            page.after,
        )
    });
    assert!(
        element_of(page).contains(Some(element_of(bar).as_ref())),
        "the bar is inside the page's content, where it was declared, not after the page"
    );

    let stops = document()
        .query_selector_all("[tabindex=\"0\"]")
        .expect("a selector");
    let order: Vec<u64> = (0..stops.length())
        .filter_map(|index| stops.item(index))
        .filter_map(|node| node.dyn_into::<web_sys::Element>().ok())
        .filter_map(|element| id_of(&element))
        .collect();
    let expected: Vec<u64> = [before, bar_button, after]
        .into_iter()
        .map(Into::into)
        .collect();
    assert_eq!(
        order, expected,
        "Tab walks the document's order: the button before the layer, the bar's, then the one after"
    );
}
