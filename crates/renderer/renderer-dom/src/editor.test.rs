//! A field and a text area drawn by a document: the browser lays out the letters while Telar places the caret, the selection and the underlines, so the two have to agree on where every letter is — along a line that scrolls under its box, down rows a fallback face would make taller, and in a view that follows the caret down.
//!
//! Needs a browser, so it runs under `wasm-bindgen-test` rather than `cargo test` — see the crate README.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::rc::Rc;

use geometry_core::{Rect, Size};
use layout_core::LayoutStyle;
use platform_core::{
    Event, EventHandler, Ime, Key, ModifiersState, NamedKey, Platform, WindowConfig,
};
use platform_web::{WebPlatform, WebPlatformConfig, WebWindow};
use reactive_core::{RwSignal, signal};
use renderer_core::{Color, DrawCommand, RenderBackend, TextStyle};
use telar_renderer_dom::{CanvasTextMetrics, DomRenderer, ID_ATTRIBUTE};
use ui_core::{
    ComponentList, Container, Input, LayoutItem, LayoutScrollArea, ScrollViewport, TextArea,
    WindowRoot, box_item,
};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

const WIDTH: f32 = 320.0;
const HEIGHT: f32 = 400.0;

fn document() -> web_sys::Document {
    web_sys::window()
        .and_then(|window| window.document())
        .expect("a document")
}

fn style() -> TextStyle {
    TextStyle::new(16.0, Color::BLACK)
}

/// The measurer, the capture and the surface a document backend runs with, before anything is built against them.
fn as_a_document() {
    renderer_core::set_text_metrics(CanvasTextMetrics);
    ui_tree::set_element_capture(true);
    ui_core::set_surface_size(Size::new(WIDTH, HEIGHT));
}

fn line() -> f32 {
    ui_core::line_box(&style())
}

fn key(key: Key) -> Event {
    Event::KeyPressed {
        key,
        modifiers: ModifiersState::default(),
        unmodified: None,
    }
}

/// A surface drawn into a host of its own, with the frames a runner would draw.
struct Page {
    host: web_sys::HtmlElement,
    renderer: DomRenderer,
    tree: ComponentList,
    last: Vec<DrawCommand>,
}

impl Page {
    fn new(content: Box<dyn LayoutItem>) -> Self {
        let column = Container::new(LayoutStyle::new().flex_column().width(WIDTH), vec![content])
            .expect("the column builds");
        let mut tree = ComponentList::new(WindowRoot::new(box_item(column)));
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
        document()
            .body()
            .expect("a body")
            .append_child(host.as_ref())
            .expect("the host went into the page");
        let renderer = DomRenderer::new(host.clone()).expect("a renderer on the host");
        let mut page = Self {
            host,
            renderer,
            tree,
            last: Vec::new(),
        };
        page.frame();
        page
    }

    fn frame(&mut self) {
        ui_core::relayout_if_dirty();
        reactive_core::batch(|| {});
        ui_core::relayout_if_dirty();
        self.last = self.tree.commands().clone();
        self.renderer
            .render_frame(&self.last, None)
            .expect("the frame reconciled");
    }

    fn send(&mut self, event: Event) {
        ui_core::observe_keyboard(&event);
        self.tree.on_event(&event);
        self.frame();
    }

    fn element(&self, node: layout_core::NodeId) -> web_sys::Element {
        self.host
            .query_selector(&format!("[{ID_ATTRIBUTE}=\"{}\"]", u64::from(node)))
            .expect("a valid selector")
            .expect("the box is in the document")
    }

    /// The caret the focused editor drew, in its own box.
    fn caret(&self) -> Rect {
        self.last
            .iter()
            .find_map(|command| match command {
                DrawCommand::PushElement { element } => element.caret,
                _ => None,
            })
            .expect("the focused editor says where its caret is")
    }
}

impl Drop for Page {
    fn drop(&mut self) {
        self.host.remove();
    }
}

/// The text node under `node` the letters are drawn in.
fn letters(node: &web_sys::Node) -> web_sys::Node {
    fn find(node: &web_sys::Node) -> Option<web_sys::Node> {
        if node.node_type() == web_sys::Node::TEXT_NODE {
            return Some(node.clone());
        }
        let children = node.child_nodes();
        (0..children.length())
            .filter_map(|index| children.item(index))
            .find_map(|child| find(&child))
    }
    find(node).expect("the editor draws its text")
}

/// Where the browser drew `text[range]`, a byte range, relative to `origin`.
fn drawn_at(
    text: &web_sys::Node,
    range: std::ops::Range<usize>,
    origin: &web_sys::Element,
) -> Rect {
    let content = text.text_content().unwrap_or_default();
    let units = |at: usize| content[..at].encode_utf16().count() as u32;
    let dom = document().create_range().expect("a range");
    dom.set_start(text, units(range.start)).expect("a start");
    dom.set_end(text, units(range.end)).expect("an end");
    let at = dom.get_bounding_client_rect();
    let base = origin.get_bounding_client_rect();
    Rect::new(
        (at.left() - base.left()) as f32,
        (at.top() - base.top()) as f32,
        at.width() as f32,
        at.height() as f32,
    )
}

/// The rects painted inside `element` as `height` px tall, relative to it.
fn painted(element: &web_sys::Element, height: f32) -> Vec<Rect> {
    let base = element.get_bounding_client_rect();
    let children = element.children();
    (0..children.length())
        .filter_map(|index| children.item(index))
        .map(|child| child.get_bounding_client_rect())
        .filter(|rect| (rect.height() as f32 - height).abs() < 0.01)
        .map(|rect| {
            Rect::new(
                (rect.left() - base.left()) as f32,
                (rect.top() - base.top()) as f32,
                rect.width() as f32,
                rect.height() as f32,
            )
        })
        .collect()
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1.0
}

const LONG: &str = "Mañana será otro día, y pasado también lo será";

/// **A field is one line.** A value longer than the box stays on its line, is cut at the box's edges and scrolls under it, so the caret at its end is in the box and drawn after the last letter the browser drew.
#[wasm_bindgen_test]
fn a_long_value_stays_on_one_line_and_scrolls_to_the_caret() {
    as_a_document();
    let value = signal(LONG.to_string());
    let input = Input::new(value, LayoutStyle::new().width(120.0).height(30.0), style)
        .expect("the field builds");
    let (node, focus) = (input.layout_node(), input.focus_id());
    let mut page = Page::new(box_item(input));
    ui_core::focus::request(focus);
    page.frame();

    let element = page.element(node);
    let text = letters(element.as_ref());
    let first = drawn_at(&text, 0..1, &element);
    let end = LONG.char_indices().last().map_or(0, |(at, _)| at);
    let last = drawn_at(&text, end..LONG.len(), &element);
    assert!(near(first.y, last.y), "one line: {first:?} and {last:?}");
    let overflow = web_sys::window()
        .and_then(|window| window.get_computed_style(&element).ok().flatten())
        .and_then(|computed| computed.get_property_value("overflow-x").ok())
        .unwrap_or_default();
    assert!(
        overflow == "clip" || overflow == "hidden",
        "the box cuts what runs past it: {overflow}"
    );
    let caret = page.caret();
    assert!(
        caret.x >= 0.0 && caret.x + caret.width <= 120.0 + 0.01,
        "the caret is in the box: {caret:?}"
    );
    assert!(
        near(caret.x, last.x + last.width),
        "and after the last letter, where the browser drew it: {caret:?} {last:?}"
    );

    page.send(key(Key::Named(NamedKey::Home)));
    let element = page.element(node);
    let text = letters(element.as_ref());
    assert!(near(page.caret().x, 0.0));
    assert!(
        near(drawn_at(&text, 0..1, &element).x, 0.0),
        "Home scrolls back to the first letter"
    );
}

/// **A composition is underlined under its own letters, on its own row.** Accented letters and a space before it, and a row of kana above it: the browser gives a row that falls back to another face a taller line unless the line is said outright, so every row under one drifted from where Telar put the caret and the underline; and it measured a run ending in a space without the space, so the underline and the caret stood a space short of the letters.
#[wasm_bindgen_test]
fn a_composition_after_accented_text_and_a_row_of_kana_is_underlined_under_its_letters() {
    as_a_document();
    let initial = "Mañana será\nにほんご\nlínea \nfin";
    let at = initial.find("\nfin").expect("the last row");
    let value = signal(initial.to_string());
    let area = TextArea::new(value, LayoutStyle::new(), style).expect("the area builds");
    let (node, focus, caret) = (area.layout_node(), area.focus_id(), area.caret());
    let mut page = Page::new(box_item(area));
    ui_core::focus::request(focus);
    caret.set(at);
    page.frame();
    let preedit = "かな";
    page.send(Event::Ime(Ime::preedit(
        preedit,
        Some((preedit.len(), preedit.len())),
    )));

    let element = page.element(node);
    let text = letters(element.as_ref());
    let last = at + preedit.len() + 1;
    let fourth = drawn_at(&text, last..last + 1, &element);
    assert!(
        near(fourth.y, 3.0 * line()) && near(fourth.height, line()),
        "the last row starts three lines down, whatever the rows above it hold: {fourth:?} for a {} line",
        line()
    );
    let composed = drawn_at(&text, at..at + preedit.len(), &element);
    let underline = painted(&element, 1.0);
    assert_eq!(
        underline.len(),
        1,
        "one line under the composition: {underline:?}"
    );
    let underline = underline[0];
    assert!(
        near(underline.x, composed.x)
            && near(underline.x + underline.width, composed.x + composed.width),
        "under its letters: {underline:?} for {composed:?}"
    );
    assert!(
        near(underline.y + underline.height, 3.0 * line()),
        "at the foot of the third row: {underline:?}"
    );
    let caret = page.caret();
    assert!(
        near(caret.x, composed.x + composed.width),
        "{caret:?} {composed:?}"
    );
    assert!(near(caret.y, 2.0 * line()), "{caret:?}");
}

/// The same in a field: kana composed after accented letters and a space are underlined where the browser drew them, with the caret after them.
#[wasm_bindgen_test]
fn kana_composed_in_a_field_after_accented_letters_and_a_space_are_marked_where_they_are_drawn() {
    as_a_document();
    let initial = "Árbol ñandú ";
    let value = signal(initial.to_string());
    let input = Input::new(value, LayoutStyle::new().width(300.0).height(30.0), style)
        .expect("the field builds");
    let (node, focus) = (input.layout_node(), input.focus_id());
    let mut page = Page::new(box_item(input));
    ui_core::focus::request(focus);
    page.frame();
    let preedit = "にほん";
    page.send(Event::Ime(Ime::preedit(
        preedit,
        Some((preedit.len(), preedit.len())),
    )));

    let element = page.element(node);
    let text = letters(element.as_ref());
    let caret = page.caret();
    let kana = drawn_at(
        &text,
        initial.len()..initial.len() + preedit.len(),
        &element,
    );
    assert!(near(caret.x, kana.x + kana.width), "{caret:?} {kana:?}");
    let underline = painted(&element, 1.0);
    assert_eq!(underline.len(), 1, "{underline:?}");
    assert!(near(underline[0].x, kana.x), "{underline:?} {kana:?}");
}

/// **The view follows the caret down.** Rows typed past the bottom of the view scroll it with them; the browser is asked once the box holds the new rows, and asked again by every drawing of the frame, not only the first.
#[wasm_bindgen_test]
fn rows_typed_past_the_bottom_of_a_text_area_scroll_the_view_with_them() {
    as_a_document();
    let made: Rc<RefCell<Option<(ui_core::focus::FocusId, ScrollViewport)>>> = Rc::default();
    let sink = Rc::clone(&made);
    let value: RwSignal<String> = signal("one".to_string());
    let scroll = LayoutScrollArea::new_with(
        LayoutStyle::new().width(WIDTH).height(line() * 3.0),
        move |viewport| {
            let area = TextArea::new(value, LayoutStyle::new(), style)?;
            *sink.borrow_mut() = Some((area.focus_id(), viewport));
            Ok(box_item(area))
        },
    )
    .expect("the scroll builds");
    let scroll_node = scroll.layout_node();
    let mut page = Page::new(box_item(scroll));
    let (focus, viewport) = made.borrow().clone().expect("the builder ran");
    ui_core::focus::request(focus);
    page.frame();
    for _ in 0..6 {
        page.send(key(Key::Named(NamedKey::Enter)));
    }

    let element = page.element(scroll_node);
    let at = element.scroll_top() as f32;
    let (_, offset) = viewport.peek_offset();
    assert!(offset > 0.0, "the view moved: {offset}");
    assert!(
        near(at, offset),
        "the browser shows what Telar scrolled to: {at} for {offset}"
    );
    let caret = page.caret();
    assert!(
        caret.y + caret.height - at <= element.client_height() as f32 + 0.5,
        "the caret's row is in view: {caret:?} at {at}"
    );
}

thread_local! {
    static RECORDED: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
    static MOUNTED: RefCell<Option<web_sys::HtmlElement>> = const { RefCell::new(None) };
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

/// The platform listening on `host`, so a press in the document reaches the tree as it does in an app.
fn listen_on(host: &web_sys::HtmlElement) {
    if MOUNTED.with(|mounted| mounted.borrow().is_some()) {
        return;
    }
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
    MOUNTED.with(|mounted| *mounted.borrow_mut() = Some(host.clone()));
}

async fn next_frames() {
    for _ in 0..2 {
        let promise = js_sys::Promise::new(&mut |resolve, _| {
            let _ = web_sys::window()
                .expect("a window")
                .request_animation_frame(&resolve);
        });
        let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
    }
}

/// Dispatches a pointer event of `kind` at `(x, y)` in `element`'s own coordinates.
fn pointer(element: &web_sys::Element, kind: &str, (x, y): (f32, f32), shift: bool) {
    let base = element.get_bounding_client_rect();
    let init = web_sys::PointerEventInit::new();
    init.set_bubbles(true);
    init.set_client_x((base.left() + x as f64) as i32);
    init.set_client_y((base.top() + y as f64) as i32);
    init.set_pointer_type("mouse");
    init.set_is_primary(true);
    init.set_shift_key(shift);
    init.set_buttons(if kind == "pointerup" { 0 } else { 1 });
    let event =
        web_sys::PointerEvent::new_with_event_init_dict(kind, &init).expect("a pointer event");
    element.dispatch_event(&event).expect("dispatched");
}

/// **A press in the document selects as it does anywhere.** A drag runs a selection across the letters it passes, a double-click takes a word, a triple-click the line, and Shift with a press extends what was selected — at the letters the browser drew, which is where the press lands.
#[wasm_bindgen_test]
async fn presses_and_drags_in_the_document_select_the_letters_under_them() {
    as_a_document();
    const WORDS: &str = "uno dos tres";
    let value = signal(WORDS.to_string());
    let input = Input::new(value, LayoutStyle::new().width(300.0).height(30.0), style)
        .expect("the field builds");
    let node = input.layout_node();
    let mut page = Page::new(box_item(input));
    listen_on(&page.host);
    let element = page.element(node);
    let text = letters(element.as_ref());
    let letters: Vec<Rect> = (0..WORDS.len())
        .map(|at| drawn_at(&text, at..at + 1, &element))
        .collect();
    let before = |at: usize| {
        (
            letters[at].x + 0.5,
            letters[at].y + letters[at].height * 0.5,
        )
    };
    let inside = |at: usize| {
        let letter = letters[at];
        (
            letter.x + letter.width * 0.5,
            letter.y + letter.height * 0.5,
        )
    };
    let replay = |page: &mut Page| {
        let events = RECORDED.with(|recorded| std::mem::take(&mut *recorded.borrow_mut()));
        for event in events {
            page.send(event);
        }
    };
    let typed_over = |page: &mut Page, c: char| {
        page.send(key(Key::Char(c)));
        let typed = value.get();
        value.set(WORDS.to_string());
        page.frame();
        typed
    };
    next_frames().await;
    replay(&mut page);

    pointer(&element, "pointerdown", before(4), false);
    pointer(&element, "pointermove", before(7), false);
    pointer(&element, "pointermove", before(10), false);
    pointer(&element, "pointerup", before(10), false);
    next_frames().await;
    replay(&mut page);
    assert_eq!(
        typed_over(&mut page, 'X'),
        "uno Xes",
        "a drag selects what it ran across"
    );

    wait_past_a_double_click().await;
    for _ in 0..2 {
        pointer(&element, "pointerdown", inside(5), false);
        pointer(&element, "pointerup", inside(5), false);
    }
    next_frames().await;
    replay(&mut page);
    assert_eq!(
        typed_over(&mut page, 'X'),
        "uno X tres",
        "a double-click takes the word"
    );

    wait_past_a_double_click().await;
    for _ in 0..3 {
        pointer(&element, "pointerdown", inside(1), false);
        pointer(&element, "pointerup", inside(1), false);
    }
    next_frames().await;
    replay(&mut page);
    assert_eq!(
        typed_over(&mut page, 'Y'),
        "Y",
        "a triple-click takes the whole line"
    );

    wait_past_a_double_click().await;
    pointer(&element, "pointerdown", before(4), false);
    pointer(&element, "pointerup", before(4), false);
    next_frames().await;
    replay(&mut page);
    wait_past_a_double_click().await;
    pointer(&element, "pointerdown", before(8), true);
    pointer(&element, "pointerup", before(8), true);
    next_frames().await;
    replay(&mut page);
    assert_eq!(
        typed_over(&mut page, 'Z'),
        "uno Ztres",
        "Shift with a press extends from the caret"
    );
}

async fn wait_past_a_double_click() {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        let _ = web_sys::window()
            .expect("a window")
            .set_timeout_with_callback_and_timeout_and_arguments_0(
                &resolve,
                ui_core::DOUBLE_CLICK.as_millis() as i32 + 50,
            );
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}
