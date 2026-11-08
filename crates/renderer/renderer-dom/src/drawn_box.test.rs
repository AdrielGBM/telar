//! A box drawn inside a drawing, on a page the document scrolls: a name scaled from its start inside a mask, on a sticky stage below the top of the page, lands where the same box laid out as an element does, a canvas's artwork inside the mask keeps its own transform, and the mask and a canvas beside it, both pinned to fill the stage, cover it.
//!
//! Needs a browser, so it runs under `wasm-bindgen-test` rather than `cargo test` — see the crate README.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;

use geometry_core::{Rect, Size};
use layout_core::{LayoutStyle, SizeDimension};
use platform_core::Event;
use renderer_core::{Color, RectStyle, RenderBackend};
use telar_renderer_dom::{CanvasTextMetrics, DomRenderer, ID_ATTRIBUTE};
use ui_core::{
    Canvas, ComponentList, Container, LayoutItem, Mask, NodeId, ScrollPage, StyledContainer,
    TransformOrigin, box_item, box_transform_about,
};
use ui_tree::RenderNode;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

const SPACER: f32 = 300.0;
const INSET: (f32, f32) = (56.0, 128.0);
const NAME: (f32, f32) = (200.0, 80.0);
const SCALE: f32 = 1.5;
const NAME_FILL: (u8, u8, u8) = (200, 30, 40);
const ART_FILL: (u8, u8, u8) = (30, 40, 200);
const COVER_FILL: (u8, u8, u8) = (40, 160, 60);

thread_local! {
    static PAGE: RefCell<Option<Page>> = const { RefCell::new(None) };
}

struct Page {
    tree: ComponentList,
    renderer: DomRenderer,
    height: f32,
    stage: NodeId,
    name: NodeId,
    mask: NodeId,
    cover: NodeId,
}

fn window() -> web_sys::Window {
    web_sys::window().expect("a window")
}

fn document() -> web_sys::Document {
    window().document().expect("a document")
}

fn fill((r, g, b): (u8, u8, u8)) -> Color {
    Color::from_rgb_u8(r, g, b)
}

fn spelled((r, g, b): (u8, u8, u8)) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// The name as the Opening sets it: scaled from its start.
fn name() -> StyledContainer {
    StyledContainer::new(
        LayoutStyle::new().width(NAME.0).height(NAME.1),
        |_| RectStyle::filled(fill(NAME_FILL), 0.0),
        vec![],
    )
    .expect("the name builds")
    .with_transform(|rect| {
        box_transform_about(rect, TransformOrigin::start(), 0.0, SCALE, SCALE, 0.0, 0.0)
    })
}

/// A column that sets what it holds `INSET` in from its corner.
fn inset(children: Vec<Box<dyn LayoutItem>>) -> Container {
    Container::new(
        LayoutStyle::new()
            .flex_column()
            .padding_left(INSET.0)
            .padding_top(INSET.1),
        children,
    )
    .expect("the column builds")
}

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

/// A page holding a spacer, then a track three screens tall whose stage sticks to the top. The stage holds the name as an element, a mask over the whole stage whose content holds the same name, at the same place, and a canvas under it, and beside the mask a canvas over the whole stage that fills all of itself, as the portfolio's Opening sets them.
fn mount() {
    if PAGE.with(|page| page.borrow().is_some()) {
        return;
    }
    let root = document().document_element().expect("a root element");
    // The surface is measured once, before the page is tall enough to scroll: the document's bar is reserved now, or layout would keep the width the bar later takes and the canvas's artwork would outrun its `<svg>`.
    let _ = root
        .dyn_ref::<web_sys::HtmlElement>()
        .expect("the root is an HTML element")
        .style()
        .set_property("overflow-y", "scroll");
    let (width, height) = (root.client_width() as f32, root.client_height() as f32);
    renderer_core::set_text_metrics(CanvasTextMetrics);
    ui_tree::set_element_capture(true);
    ui_core::set_surface_size(Size::new(width, height));

    let html_name = name();
    let name_node = html_name.layout_node();
    let art = Canvas::new(LayoutStyle::new().width(100.0).height(50.0), |_| {
        RenderNode::transform_with(
            [2.0, 0.0, 0.0, 2.0, 0.0, 0.0],
            [RenderNode::rect(
                Rect::new(10.0, 10.0, 20.0, 20.0),
                RectStyle::filled(fill(ART_FILL), 0.0),
            )],
        )
    })
    .expect("the canvas builds");
    let source = StyledContainer::new(
        LayoutStyle::new()
            .width(SizeDimension::Percent(1.0))
            .height(SizeDimension::Percent(1.0)),
        |_| RectStyle::filled(Color::WHITE, 0.0),
        vec![],
    )
    .expect("the source builds");
    let mask = Mask::new(
        LayoutStyle::new().absolute_fill(),
        box_item(source),
        box_item(inset(vec![box_item(name()), box_item(art)])),
    )
    .expect("the mask builds");
    let mask_node = mask.layout_node();
    let cover = Canvas::new(LayoutStyle::new().absolute_fill(), |rect| {
        RenderNode::rect(rect, RectStyle::filled(fill(COVER_FILL), 0.0))
    })
    .expect("the cover builds");
    let cover_node = cover.layout_node();
    let stage = StyledContainer::new(
        LayoutStyle::new()
            .sticky()
            .inset_top(0.0)
            .height(height)
            .flex_column(),
        |_| RectStyle::filled(Color::from_rgb_u8(240, 240, 240), 0.0),
        vec![
            box_item(inset(vec![box_item(html_name)])),
            box_item(mask),
            box_item(cover),
        ],
    )
    .expect("the stage builds");
    let stage_node = stage.layout_node();

    let mut page = ScrollPage::new_with(|_| {
        let track = Container::new(
            LayoutStyle::new().flex_column().height(height * 3.0),
            vec![box_item(stage)],
        )?;
        let content = Container::new(
            LayoutStyle::new()
                .flex_column()
                .width(SizeDimension::Percent(1.0)),
            vec![
                box_item(Container::new(LayoutStyle::new().height(SPACER), vec![])?),
                box_item(track),
                box_item(Container::new(LayoutStyle::new().height(height), vec![])?),
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
            height,
            stage: stage_node,
            name: name_node,
            mask: mask_node,
            cover: cover_node,
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

fn shown(element: &web_sys::Element) -> Rect {
    let rect = element.get_bounding_client_rect();
    Rect::new(
        rect.x() as f32,
        rect.y() as f32,
        rect.width() as f32,
        rect.height() as f32,
    )
}

/// The shape in `drawing`'s picture filled with `color`.
fn drawn_in(drawing: NodeId, color: (u8, u8, u8)) -> Rect {
    let shape = element_of(drawing)
        .query_selector(&format!("[fill=\"{}\"]", spelled(color)))
        .expect("a selector")
        .unwrap_or_else(|| panic!("the drawing draws a shape filled {}", spelled(color)));
    shown(&shape)
}

fn assert_rect(case: &str, got: Rect, expected: Rect) {
    for (edge, expected, got) in [
        ("x", expected.x, got.x),
        ("y", expected.y, got.y),
        ("width", expected.width, got.width),
        ("height", expected.height, got.height),
    ] {
        assert!(
            (expected - got).abs() < 1.0,
            "{case}: {edge} should be {expected}, the browser drew {got}"
        );
    }
}

/// The name as both copies should show it, and the canvas's square, from where the browser put the stage.
fn expected() -> (Rect, Rect) {
    let stage = shown(&element_of(with_page(|page| page.stage)));
    let (x, y) = (stage.x + INSET.0, stage.y + INSET.1);
    let name = Rect::new(
        x,
        y + NAME.1 * (1.0 - SCALE) / 2.0,
        NAME.0 * SCALE,
        NAME.1 * SCALE,
    );
    let art = Rect::new(x + 20.0, y + NAME.1 + 20.0, 40.0, 40.0);
    (name, art)
}

fn assert_both_copies_agree(case: &str) {
    let (name, art) = expected();
    let mask = with_page(|page| page.mask);
    let html = shown(&element_of(with_page(|page| page.name)));
    assert_rect(&format!("{case}, the element"), html, name);
    assert_rect(
        &format!("{case}, the copy in the mask"),
        drawn_in(mask, NAME_FILL),
        name,
    );
    assert_rect(
        &format!("{case}, the canvas in the mask"),
        drawn_in(mask, ART_FILL),
        art,
    );
}

/// The mask and the canvas beside it are `<svg>`s, which a browser does not stretch between their insets: each covers the stage only because its size is written down.
fn assert_both_drawings_cover_the_stage(case: &str) {
    let (stage, mask, cover) = with_page(|page| (page.stage, page.mask, page.cover));
    let stage = shown(&element_of(stage));
    assert_rect(
        &format!("{case}, the mask"),
        shown(&element_of(mask)),
        stage,
    );
    assert_rect(
        &format!("{case}, the canvas"),
        shown(&element_of(cover)),
        stage,
    );
    assert_rect(
        &format!("{case}, the canvas's artwork"),
        drawn_in(cover, COVER_FILL),
        stage,
    );
}

async fn next_frames() {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        let _ = window().set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 50);
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

#[wasm_bindgen_test]
async fn a_scaled_box_in_a_mask_lands_on_its_element_below_the_top_and_under_a_sticky_stage() {
    window().scroll_to_with_x_and_y(0.0, 0.0);
    render();
    next_frames().await;
    let stage_top = shown(&element_of(with_page(|page| page.stage))).y;
    assert!(
        (stage_top - SPACER).abs() < 1.0,
        "the stage starts below the top of the page: {stage_top}"
    );
    assert_both_copies_agree("at rest");
    assert_both_drawings_cover_the_stage("at rest");

    let height = with_page(|page| page.height);
    window().scroll_to_with_x_and_y(0.0, (SPACER + height * 1.5) as f64);
    next_frames().await;
    render();
    next_frames().await;
    let stage_top = shown(&element_of(with_page(|page| page.stage))).y;
    assert!(
        stage_top.abs() < 1.0,
        "the stage is stuck at the top: {stage_top}"
    );
    assert_both_copies_agree("stuck");
    assert_both_drawings_cover_the_stage("stuck");
}
