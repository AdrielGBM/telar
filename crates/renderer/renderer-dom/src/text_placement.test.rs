//! A text stretched taller than its line, as a text leaf always is across a row: the line sits where the frame drew it inside the box, with the band from the baseline to the cap height in the middle of the box, rather than at the top of the element the document made of the box.
//!
//! Needs a browser, so it runs under `wasm-bindgen-test` rather than `cargo test` — see the crate README.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;

use geometry_core::{Rect, Size};
use layout_core::{AlignItems, JustifyContent, LayoutStyle};
use platform_core::Event;
use renderer_core::{
    Color, DrawCommand, RectStyle, RenderBackend, TextMetrics, TextStyle, TextWrap,
};
use telar_renderer_dom::{CanvasTextMetrics, DomRenderer, ID_ATTRIBUTE};
use ui_core::focus::Role;
use ui_core::{
    Clip, ClippedItem, ComponentList, Container, Input, LayoutItem, NodeId, StyledContainer, Text,
    WindowRoot, box_item,
};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

const WIDTH: f32 = 480.0;
const HEIGHT: f32 = 240.0;
const BAR: f32 = 40.0;
const TRIGGER: f32 = 32.0;
const ROW: f32 = 31.0;
const LABELS: [&str; 5] = ["Setup", "Simulation", "HOME", "xo", "gyp"];
const PARAGRAPH: &str = "Name regions and say what the model is made of";
const PARAGRAPH_WIDTH: f32 = 150.0;
const PARAGRAPH_BOX: f32 = 81.0;
const TOLERANCE: f64 = 1.0;
const SNAP: f32 = 0.5 + 0.01;

thread_local! {
    static PAGE: RefCell<Option<Page>> = const { RefCell::new(None) };
}

struct Page {
    host: web_sys::HtmlElement,
    tree: ComponentList,
    renderer: DomRenderer,
    title: NodeId,
    label: NodeId,
    labels: Vec<NodeId>,
    paragraph: NodeId,
    field_label: NodeId,
    field: NodeId,
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

    let labels: Vec<Text> = LABELS.into_iter().map(line).collect();
    let label_nodes = labels.iter().map(|label| label.layout_node()).collect();
    let row = Container::new(
        LayoutStyle::new().flex_row().gap(8.0).height(ROW),
        labels.into_iter().map(box_item).collect(),
    )
    .expect("the row builds");

    let paragraph = Text::new(
        || PARAGRAPH.to_string(),
        LayoutStyle::new()
            .width(PARAGRAPH_WIDTH)
            .height(PARAGRAPH_BOX),
        || TextStyle::new(13.0, Color::BLACK),
    )
    .expect("the paragraph builds");
    let paragraph_node = paragraph.layout_node();

    let field_style = || TextStyle::new(14.0, Color::BLACK).with_font_weight(600);
    let field_label = Text::new(|| "Name".to_string(), LayoutStyle::new(), field_style)
        .expect("the label builds");
    let field_label_node = field_label.layout_node();
    let field = Input::new(
        reactive_core::signal("Ada".to_string()),
        LayoutStyle::new().width(200.0),
        field_style,
    )
    .expect("the field builds");
    let field_node = field.layout_node();
    let form = Container::new(
        LayoutStyle::new().flex_row().gap(8.0).height(ROW),
        vec![box_item(field_label), box_item(field)],
    )
    .expect("the form row builds");

    let page = Container::new(
        LayoutStyle::new()
            .flex_column()
            .align_items(AlignItems::START),
        vec![
            box_item(bar),
            box_item(row),
            box_item(paragraph),
            box_item(form),
        ],
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
            labels: label_nodes,
            paragraph: paragraph_node,
            field_label: field_label_node,
            field: field_node,
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

/// Where layout put the box, where inside it the frame drew its line, and the style it drew the line in.
fn drawn(node: NodeId) -> (Rect, Rect, TextStyle) {
    let id = u64::from(node);
    with_page(|page| {
        let commands = page.tree.commands();
        let mut inside = None;
        for command in commands.iter() {
            match command {
                DrawCommand::PushElement { element } if element.id.0 == id => {
                    inside = Some(element.rect);
                }
                DrawCommand::Text { rect, style, .. } if inside.is_some() => {
                    return (inside.expect("inside the box"), *rect, (**style).clone());
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
    let (box_rect, line, _) = drawn(node);
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

/// How far the middle of the span from the first line's cap height to the last line's baseline is from the middle of the box, measured by the engine that draws.
fn cap_band_off_middle(node: NodeId) -> f32 {
    render();
    let (box_rect, line, style) = drawn(node);
    let (top, height) = CanvasTextMetrics.ink_bounds("H", 1_000.0, &style);
    let (_, one_line) = CanvasTextMetrics.measure("H", None, 1_000.0, &style);
    assert!(height > 0.0, "the capital inks");
    let lines = (line.height / one_line).round();
    let middle = line.y + top + ((lines - 1.0) * one_line + height) / 2.0;
    middle - box_rect.height / 2.0
}

#[wasm_bindgen_test]
fn a_title_cut_at_its_ends_sits_in_the_middle_of_the_bar() {
    let title = with_page(|page| page.title);
    render();
    let (box_rect, line, _) = drawn(title);
    assert!(
        line.y > 0.0,
        "the title is stretched across the bar, taller than its line: {box_rect:?} holds {line:?}"
    );
    assert_where_drawn(title, "the title");
    let off = cap_band_off_middle(title);
    assert!(
        off.abs() <= SNAP,
        "the title's cap band is {off}px off the bar's middle"
    );
}

#[wasm_bindgen_test]
fn a_button_label_sits_in_the_middle_of_the_button() {
    let label = with_page(|page| page.label);
    render();
    let (box_rect, line, _) = drawn(label);
    assert!(
        line.y > 0.0,
        "the label is stretched across the button, taller than its line: {box_rect:?} holds {line:?}"
    );
    assert_where_drawn(label, "the label");
    let off = cap_band_off_middle(label);
    assert!(
        off.abs() <= SNAP,
        "the label's cap band is {off}px off the button's middle"
    );
}

#[wasm_bindgen_test]
fn a_row_of_labels_shares_one_baseline_whatever_letters_they_have() {
    let labels = with_page(|page| page.labels.clone());
    render();
    let tops: Vec<f64> = labels.iter().map(|&label| laid_out(label).top()).collect();
    assert!(
        tops.iter().all(|top| (top - tops[0]).abs() < 0.01),
        "the browser lays every label's line at one top: {:?}",
        LABELS.iter().zip(&tops).collect::<Vec<_>>()
    );
    for (&label, what) in labels.iter().zip(LABELS) {
        assert_where_drawn(label, what);
        let off = cap_band_off_middle(label);
        assert!(
            off.abs() <= SNAP,
            "{what}: the cap band is {off}px off the row's middle"
        );
    }
}

#[wasm_bindgen_test]
fn several_lines_centre_the_span_from_the_first_cap_height_to_the_last_baseline() {
    let paragraph = with_page(|page| page.paragraph);
    render();
    let (_, line, style) = drawn(paragraph);
    let (_, one_line) = CanvasTextMetrics.measure("H", None, 1_000.0, &style);
    assert!(
        line.height >= 2.0 * one_line - 0.01,
        "the paragraph wraps: {line:?}"
    );
    assert_where_drawn(paragraph, "the paragraph");
    let off = cap_band_off_middle(paragraph);
    assert!(
        off.abs() <= SNAP,
        "the paragraph's band is {off}px off its box's middle"
    );
}

#[wasm_bindgen_test]
fn descenders_stay_inside_the_box() {
    let labels = with_page(|page| page.labels.clone());
    render();
    for (&label, what) in labels.iter().zip(LABELS) {
        let (box_rect, line, style) = drawn(label);
        let (top, height) = CanvasTextMetrics.ink_bounds(what, 1_000.0, &style);
        assert!(
            line.y + top >= 0.0 && line.y + top + height <= box_rect.height,
            "{what}: inks from {} to {} in a {}px box",
            line.y + top,
            line.y + top + height,
            box_rect.height
        );
    }
}

#[wasm_bindgen_test]
fn a_field_and_a_label_beside_it_share_the_cap_band() {
    let (label, field) = with_page(|page| (page.field_label, page.field));
    render();
    let (_, label_line, _) = drawn(label);
    let (_, field_line, _) = drawn(field);
    assert_eq!(
        (label_line.y, label_line.height),
        (field_line.y, field_line.height),
        "the label and the field draw their lines alike"
    );
    let (label_top, field_top) = (laid_out(label).top(), laid_out(field).top());
    assert!(
        (label_top - field_top).abs() < 0.01,
        "the browser lays both lines at one top: {label_top} and {field_top}"
    );
    assert_where_drawn(field, "the field");
    let off = cap_band_off_middle(field);
    assert!(
        off.abs() <= SNAP,
        "the field's cap band is {off}px off its middle"
    );
}
