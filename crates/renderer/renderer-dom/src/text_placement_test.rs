//! A text stretched taller than its line, as a text leaf always is across a row: the line sits where the frame drew it inside the box, centred, rather than at the top of the element the document made of the box. A bar's title cut at its ends and a button's label, both in a row that centres what it holds, came out at the top of their boxes in the browser while every other target drew them in the middle.
//!
//! Needs a browser, so it runs under `wasm-bindgen-test` rather than `cargo test` — see the crate README.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;

use geometry_core::{Rect, Size};
use layout_core::{AlignItems, JustifyContent, LayoutStyle};
use platform_core::Event;
use renderer_core::{Color, DrawCommand, RectStyle, RenderBackend, TextWrap};
use telar_renderer_dom::{CanvasTextMetrics, DomRenderer, ID_ATTRIBUTE};
use ui_core::focus::Role;
use ui_core::{
    Clip, ClippedItem, ComponentList, Container, LayoutItem, NodeId, StyledContainer, Text,
    WindowRoot, box_item,
};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

const WIDTH: f32 = 480.0;
const HEIGHT: f32 = 120.0;
const BAR: f32 = 40.0;
const TRIGGER: f32 = 32.0;
const TOLERANCE: f64 = 1.0;

thread_local! {
    static PAGE: RefCell<Option<Page>> = const { RefCell::new(None) };
}

struct Page {
    host: web_sys::HtmlElement,
    tree: ComponentList,
    renderer: DomRenderer,
    title: NodeId,
    label: NodeId,
}

fn document() -> web_sys::Document {
    web_sys::window()
        .and_then(|window| window.document())
        .expect("a document")
}

fn line(text: &'static str) -> Text {
    Text::declaring(
        move || text.to_string(),
        LayoutStyle::new().min_width(0.0),
        |style| style.with_font_weight(600).with_text_wrap(TextWrap::NoWrap),
    )
    .expect("the text builds")
}

fn mount() {
    if PAGE.with(|page| page.borrow().is_some()) {
        return;
    }
    renderer_core::set_text_metrics(CanvasTextMetrics);
    ui_tree::set_element_capture(true);
    ui_core::set_surface_size(Size::new(WIDTH, HEIGHT));

    let title = line("Workshop");
    let title_node = title.layout_node();
    let label = line("Canvas settings");
    let label_node = label.layout_node();
    let trigger = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .height(TRIGGER)
            .padding_horizontal(12.0),
        |_| RectStyle::filled(Color::from_rgb_u8(220, 220, 230), 6.0),
        vec![box_item(label)],
    )
    .expect("the trigger builds")
    .control(Role::Button);
    let bar = Container::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .gap(8.0)
            .padding_horizontal(12.0)
            .height(BAR),
        vec![
            box_item(ClippedItem::new(box_item(title), Clip::x())),
            box_item(trigger),
        ],
    )
    .expect("the bar builds");

    let mut tree = ComponentList::new(WindowRoot::new(box_item(bar)));
    tree.on_event(&Event::WindowResized {
        width: WIDTH as u32,
        height: HEIGHT as u32,
    });
    let host: web_sys::HtmlElement = document()
        .create_element("div")
        .expect("a host element")
        .dyn_into()
        .expect("a div is an HTML element");
    host.style()
        .set_property("width", &format!("{WIDTH}px"))
        .expect("the host takes a width");
    host.style()
        .set_property("height", &format!("{HEIGHT}px"))
        .expect("the host takes a height");
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
            title: title_node,
            label: label_node,
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

/// Where layout put the box, and where inside it the frame drew its line.
fn drawn(node: NodeId) -> (Rect, Rect) {
    let id = u64::from(node);
    with_page(|page| {
        let commands = page.tree.commands();
        let mut inside = None;
        for command in commands.iter() {
            match command {
                DrawCommand::PushElement { element } if element.id.0 == id => {
                    inside = Some(element.rect);
                }
                DrawCommand::Text { rect, .. } if inside.is_some() => {
                    return (inside.expect("inside the box"), *rect);
                }
                DrawCommand::PopElement if inside.is_some() => break,
                _ => {}
            }
        }
        panic!("box {id} draws a line of text")
    })
}

/// Where the browser laid the glyphs of the box's text out, against the host.
fn laid_out(node: NodeId) -> web_sys::DomRect {
    let id = u64::from(node);
    with_page(|page| {
        let element = page
            .host
            .query_selector(&format!("[{ID_ATTRIBUTE}=\"{id}\"]"))
            .expect("a valid selector")
            .unwrap_or_else(|| panic!("box {id} is in the document"));
        let range = document().create_range().expect("a range");
        range
            .select_node_contents(&element)
            .expect("the range spans the box's text");
        range.get_bounding_client_rect()
    })
}

fn assert_where_drawn(node: NodeId, what: &str) {
    render();
    let (box_rect, line) = drawn(node);
    let host = with_page(|page| page.host.get_bounding_client_rect());
    let glyphs = laid_out(node);
    let expected_middle = host.top() + f64::from(box_rect.y + line.y + line.height / 2.0);
    let middle = glyphs.top() + glyphs.height() / 2.0;
    assert!(
        (middle - expected_middle).abs() <= TOLERANCE,
        "{what}: the line's middle is at {middle}, drawn at {expected_middle} in a box at {box_rect:?} with the line at {line:?}"
    );
    let expected_left = host.left() + f64::from(box_rect.x + line.x);
    assert!(
        (glyphs.left() - expected_left).abs() <= TOLERANCE,
        "{what}: the line starts at {}, drawn at {expected_left}",
        glyphs.left()
    );
}

#[wasm_bindgen_test]
fn a_title_cut_at_its_ends_sits_in_the_middle_of_the_bar() {
    let title = with_page(|page| page.title);
    render();
    let (box_rect, line) = drawn(title);
    assert!(
        line.y > 0.0,
        "the title is stretched across the bar, taller than its line: {box_rect:?} holds {line:?}"
    );
    assert_where_drawn(title, "the title");
}

#[wasm_bindgen_test]
fn a_button_label_sits_in_the_middle_of_the_button() {
    let label = with_page(|page| page.label);
    render();
    let (box_rect, line) = drawn(label);
    assert!(
        line.y > 0.0,
        "the label is stretched across the button, taller than its line: {box_rect:?} holds {line:?}"
    );
    assert_where_drawn(label, "the label");
}
