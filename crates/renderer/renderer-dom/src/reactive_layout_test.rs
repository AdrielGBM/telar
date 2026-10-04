//! A box whose `sticky` or `clip` follows state, switched on a live page: the document has to follow the switch on the element it already has, and without anything moving to tell it so.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;

use geometry_core::Size;
use layout_core::LayoutStyle;
use platform_core::Event;
use reactive_core::{RwSignal, signal};
use renderer_core::{Color, RectStyle, RenderBackend};
use telar_renderer_dom::{CanvasTextMetrics, DomRenderer, ID_ATTRIBUTE};
use ui_core::{
    ClippedItem, ComponentList, Container, LayoutItem, NodeId, StyledContainer, WindowRoot,
    box_item,
};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

const WIDTH: f32 = 400.0;
const HEIGHT: f32 = 300.0;

thread_local! {
    static PAGE: RefCell<Option<Page>> = const { RefCell::new(None) };
}

struct Page {
    host: web_sys::HtmlElement,
    tree: ComponentList,
    renderer: DomRenderer,
    pinned: RwSignal<bool>,
    cut: RwSignal<bool>,
    stage: NodeId,
    strip: NodeId,
}

fn document() -> web_sys::Document {
    web_sys::window()
        .and_then(|window| window.document())
        .expect("a document")
}

fn stage_style(pinned: bool) -> LayoutStyle {
    LayoutStyle::new()
        .sticky_when(pinned)
        .inset_top(10.0)
        .height(40.0)
}

fn filled() -> RectStyle {
    RectStyle::filled(Color::from_rgb_u8(30, 90, 200), 0.0)
}

fn mount() {
    if PAGE.with(|page| page.borrow().is_some()) {
        return;
    }
    renderer_core::set_text_metrics(CanvasTextMetrics);
    ui_tree::set_element_capture(true);
    ui_core::set_surface_size(Size::new(WIDTH, HEIGHT));
    let pinned = signal(true);
    let cut = signal(true);

    let stage = StyledContainer::new(stage_style(true), |_| filled(), vec![])
        .expect("the stage builds")
        .styled_by(move || stage_style(pinned.get()));
    let stage_node = stage.layout_node();
    let track = Container::new(
        LayoutStyle::new().flex_column().height(600.0),
        vec![box_item(stage)],
    )
    .expect("the track builds");

    let overflow = StyledContainer::new(
        LayoutStyle::new()
            .width(300.0)
            .height(20.0)
            .flex_shrink(0.0),
        |_| filled(),
        vec![],
    )
    .expect("the overflow builds");
    let strip = StyledContainer::new(
        LayoutStyle::new().flex_row().width(100.0).height(30.0),
        |_| RectStyle::default(),
        vec![box_item(overflow)],
    )
    .expect("the strip builds");
    let strip_node = strip.layout_node();
    let clipped = ClippedItem::following(box_item(strip), move || cut.get());

    let page = Container::new(
        LayoutStyle::new().flex_column().gap(12.0).padding_all(16.0),
        vec![box_item(clipped), box_item(track)],
    )
    .expect("the page builds");
    let mut tree = ComponentList::new(WindowRoot::new(box_item(page)));
    tree.on_event(&Event::WindowResized {
        width: WIDTH as u32,
        height: HEIGHT as u32,
    });
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
    let renderer = DomRenderer::new(host.clone()).expect("a renderer on the host");
    PAGE.with(|page| {
        *page.borrow_mut() = Some(Page {
            host,
            tree,
            renderer,
            pinned,
            cut,
            stage: stage_node,
            strip: strip_node,
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
    mount();
    PAGE.with(|page| f(page.borrow().as_ref().expect("mounted")))
}

fn element_of(node: NodeId) -> web_sys::Element {
    let id = u64::from(node);
    with_page(|page| {
        page.host
            .query_selector(&format!("[{ID_ATTRIBUTE}=\"{id}\"]"))
            .expect("a valid selector")
            .unwrap_or_else(|| panic!("box {id} is in the document"))
    })
}

fn computed(element: &web_sys::Element, property: &str) -> String {
    web_sys::window()
        .expect("a window")
        .get_computed_style(element)
        .expect("a computed style")
        .expect("styles for an element in the document")
        .get_property_value(property)
        .expect("a property value")
}

fn declared(element: &web_sys::Element) -> String {
    element.get_attribute("style").unwrap_or_default()
}

/// Off, the box is an ordinary one in the flow: not `position: sticky`, and its inset not written either, or the browser would read it as a relative offset. Nothing moves when it switches at rest, so the rect alone would never have told the document.
#[wasm_bindgen_test]
fn sticky_switches_on_the_element_it_already_has() {
    render();
    let stage = with_page(|page| page.stage);
    let before = element_of(stage);
    assert_eq!(computed(&before, "position"), "sticky");
    assert!(
        declared(&before).contains("top:10px"),
        "{}",
        declared(&before)
    );

    with_page(|page| page.pinned.set(false));
    render();
    let after = element_of(stage);
    assert!(before.is_same_node(Some(&after)), "the element was kept");
    assert_eq!(computed(&after, "position"), "relative");
    assert!(!declared(&after).contains("top:"), "{}", declared(&after));

    with_page(|page| page.pinned.set(true));
    render();
    assert_eq!(computed(&element_of(stage), "position"), "sticky");
}

/// The cut belongs to the clipped box itself, not to whatever holds it, and switching it leaves the element in place.
#[wasm_bindgen_test]
fn clip_switches_on_the_clipped_box_itself() {
    render();
    let strip = with_page(|page| page.strip);
    let before = element_of(strip);
    assert_eq!(computed(&before, "overflow-x"), "clip");
    let holder = before.parent_element().expect("the strip has a parent");
    assert_eq!(
        computed(&holder, "overflow-x"),
        "visible",
        "the parent is not what is cut"
    );

    with_page(|page| page.cut.set(false));
    render();
    let after = element_of(strip);
    assert!(before.is_same_node(Some(&after)), "the element was kept");
    assert_eq!(computed(&after, "overflow-x"), "visible");

    with_page(|page| page.cut.set(true));
    render();
    assert_eq!(computed(&element_of(strip), "overflow-x"), "clip");
}
