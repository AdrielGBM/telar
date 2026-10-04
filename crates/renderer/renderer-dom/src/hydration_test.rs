//! A page served prerendered is taken over in place: the first frame keeps every element the page was served with that is still the box it names, patches what differs where it stands, and rebuilds only a subtree that no longer fits. Needs a browser, so it runs under `wasm-bindgen-test` rather than `cargo test`.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::sync::Arc;

use geometry_core::Rect;
use platform_core::{
    Destination, Event, EventHandler, Key, Location, NamedKey, Platform, WindowConfig,
};
use platform_web::{WebPlatform, WebPlatformConfig, WebWindow};
use renderer_core::{
    Color, Declared, DrawCommand, Element, ElementId, Focusable, RectStyle, RenderBackend, Role,
    Semantics, Span, TextStyle,
};
use telar_renderer_dom::{DomRenderer, ID_ATTRIBUTE, prerender};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

thread_local! {
    static RECORDED: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
    static SURFACE: RefCell<Option<Surface>> = const { RefCell::new(None) };
}

struct Surface {
    host: web_sys::HtmlElement,
    renderer: Option<DomRenderer>,
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

fn window() -> web_sys::Window {
    web_sys::window().expect("a window")
}

fn document() -> web_sys::Document {
    window().document().expect("a document")
}

/// One host for the whole page, first in a margin-less body as the generated page has it, because the platform's listeners are per page.
fn host() -> web_sys::HtmlElement {
    if let Some(host) = SURFACE.with(|surface| surface.borrow().as_ref().map(|s| s.host.clone())) {
        return host;
    }
    let body = document().body().expect("a body");
    let _ = body.style().set_property("margin", "0");
    let host: web_sys::HtmlElement = document()
        .create_element("div")
        .expect("a host element")
        .dyn_into()
        .expect("a div is an HTML element");
    body.insert_before(host.as_ref(), body.first_child().as_ref())
        .expect("the host went into the page");
    let config = WebPlatformConfig {
        host: None,
        autofocus: false,
        owns_gestures: false,
        owns_scroll: true,
        owns_context_menu: false,
        owns_keyboard: false,
        location: None,
    };
    WebPlatform::with_host(host.clone(), config)
        .run(WindowConfig::default(), Recorder)
        .expect("the platform mounted");
    SURFACE.with(|surface| {
        *surface.borrow_mut() = Some(Surface {
            host: host.clone(),
            renderer: None,
        })
    });
    host
}

/// Writes `commands` as a page and serves it in the host, as a browser parses it before the app has loaded. Whatever renderer took over the last page goes first.
fn serve(commands: &[DrawCommand], clear: Option<Color>) -> web_sys::HtmlElement {
    let host = host();
    SURFACE.with(|surface| {
        if let Some(surface) = surface.borrow_mut().as_mut() {
            surface.renderer = None;
        }
    });
    window().scroll_to_with_x_and_y(0.0, 0.0);
    for name in ["style", "data-telar", "data-telar-document-scroll"] {
        let _ = host.remove_attribute(name);
    }
    let written = prerender(commands, clear);
    for (name, value) in &written.host_attributes {
        host.set_attribute(name, value)
            .expect("a host attribute the browser takes");
    }
    host.set_inner_html(&written.markup);
    host
}

/// Starts a renderer on the served host and draws `commands` as its first frame.
fn take_over(commands: &[DrawCommand], clear: Option<Color>) {
    let host = host();
    let mut renderer = DomRenderer::new(host).expect("a renderer on the host");
    renderer
        .render_frame(commands, clear)
        .expect("the frame reconciled");
    SURFACE.with(|surface| {
        if let Some(surface) = surface.borrow_mut().as_mut() {
            surface.renderer = Some(renderer);
        }
    });
}

/// What `commands` become in a host nobody served anything, which is what a taken-over page has to end up as.
fn built_from_nothing(commands: &[DrawCommand], clear: Option<Color>) -> String {
    let fresh: web_sys::HtmlElement = document()
        .create_element("div")
        .expect("a host element")
        .dyn_into()
        .expect("a div is an HTML element");
    document()
        .body()
        .expect("a body")
        .append_child(fresh.as_ref())
        .expect("the host went into the page");
    let mut renderer = DomRenderer::new(fresh.clone()).expect("a renderer on the host");
    renderer
        .render_frame(commands, clear)
        .expect("the frame reconciled");
    let markup = without_entry(&fresh);
    drop(renderer);
    fresh.remove();
    markup
}

/// The host's content without the editable element the reconcile keeps for typing, which a page is served without.
fn without_entry(host: &web_sys::HtmlElement) -> String {
    let copy: web_sys::Element = host
        .clone_node_with_deep(true)
        .expect("the host clones")
        .dyn_into()
        .expect("a clone of an element is one");
    if let Ok(Some(entry)) = copy.query_selector(":scope > textarea[data-telar-entry]") {
        entry.remove();
    }
    copy.inner_html()
}

fn element(id: u64) -> web_sys::Element {
    host()
        .query_selector(&format!("[{ID_ATTRIBUTE}=\"{id}\"]"))
        .expect("a valid selector")
        .unwrap_or_else(|| panic!("box {id} is in the document"))
}

fn same(a: &web_sys::Node, b: &web_sys::Node) -> bool {
    a.is_same_node(Some(b))
}

fn open(element: Element) -> DrawCommand {
    DrawCommand::PushElement {
        element: Arc::new(element),
    }
}

fn boxed(id: u64, semantics: Semantics, layout: &str, rect: Rect) -> DrawCommand {
    open(Element::new(ElementId(id), semantics, layout, rect))
}

fn text(content: &str, rect: Rect) -> DrawCommand {
    DrawCommand::Text {
        text: Arc::from(content),
        spans: None,
        rect,
        style: Arc::new(TextStyle::new(16.0, Color::rgb(0.1, 0.1, 0.1))),
    }
}

fn fill(rect: Rect, color: Color) -> DrawCommand {
    DrawCommand::Rect {
        rect,
        style: Arc::new(RectStyle::filled(color, 4.0)),
    }
}

const MAIN: u64 = 1;
const HEADING: u64 = 2;
const LINK: u64 = 3;
const BUTTON: u64 = 4;
const PARAGRAPH: u64 = 5;
const DRAWING: u64 = 6;
const STAMP: u64 = 7;

/// What can change between the page that was written and the frame that takes it over.
struct Variant {
    heading: &'static str,
    gap: &'static str,
    button_label: &'static str,
    link: Role,
    link_holds_stamp: bool,
}

const WRITTEN: Variant = Variant {
    heading: "Hola & <bienvenidos>",
    gap: "gap:8px;",
    button_label: "Cambiar tema",
    link: Role::Link,
    link_holds_stamp: false,
};

/// A page with every kind of thing a host holds: paint at its own level, boxes inside boxes, a box's own text, a paragraph cut at its spans, paint inside a box that is not a box, and a drawing.
fn page(variant: &Variant) -> Vec<DrawCommand> {
    let full = Rect::new(0.0, 0.0, 640.0, 480.0);
    let paragraph = Rect::new(0.0, 120.0, 640.0, 20.0);
    let link_semantics = match variant.link {
        Role::Link => Semantics::of(Role::Link)
            .linking_to(Destination::Route(Location::root().segment("proyectos")))
            .focusable(Focusable {
                tab_stop: true,
                consumes: Role::Link.consumed_keys(),
            }),
        role => Semantics::of(role),
    };
    let mut commands = vec![
        fill(Rect::new(0.0, 0.0, 640.0, 40.0), Color::rgb(0.9, 0.9, 0.95)),
        boxed(
            MAIN,
            Semantics::of(Role::Main).with_lang("es"),
            &format!("display:flex;flex-direction:column;{}", variant.gap),
            full,
        ),
        boxed(
            HEADING,
            Semantics::of(Role::Heading(1)).with_anchor("inicio"),
            "",
            Rect::new(0.0, 0.0, 640.0, 40.0),
        ),
        text(variant.heading, Rect::new(0.0, 0.0, 640.0, 40.0)),
        DrawCommand::PopElement,
        boxed(LINK, link_semantics, "", Rect::new(0.0, 48.0, 120.0, 20.0)),
    ];
    if variant.link_holds_stamp {
        commands.extend([
            boxed(
                STAMP,
                Semantics::group(),
                "width:10px;height:10px;",
                Rect::new(0.0, 48.0, 10.0, 10.0),
            ),
            DrawCommand::PopElement,
        ]);
    } else {
        commands.push(text("Proyectos", Rect::new(0.0, 48.0, 120.0, 20.0)));
    }
    commands.extend([
        DrawCommand::PopElement,
        boxed(
            BUTTON,
            Semantics::of(Role::Button)
                .with_label(variant.button_label)
                .focusable(Focusable {
                    tab_stop: true,
                    consumes: Role::Button.consumed_keys(),
                }),
            "width:40px;height:40px;",
            Rect::new(0.0, 76.0, 40.0, 40.0),
        ),
        fill(Rect::new(0.0, 76.0, 40.0, 40.0), Color::rgb(0.2, 0.3, 0.8)),
        fill(Rect::new(10.0, 86.0, 20.0, 20.0), Color::WHITE),
        DrawCommand::PopElement,
        boxed(PARAGRAPH, Semantics::group(), "", paragraph),
        DrawCommand::Text {
            text: Arc::from("Lee la guía completa"),
            spans: Some(Arc::from(vec![
                Span::new(0..3, Declared::default().with_font_weight(700)),
                Span::new(7..20, Declared::default())
                    .linking_to(Destination::external("https://example.com/guia").unwrap()),
            ])),
            rect: paragraph,
            style: Arc::new(TextStyle::new(16.0, Color::BLACK)),
        },
        DrawCommand::PopElement,
        boxed(
            DRAWING,
            Semantics::drawing().with_label("Un círculo"),
            "width:40px;height:40px;",
            Rect::new(0.0, 356.0, 40.0, 40.0),
        ),
        fill(Rect::new(0.0, 356.0, 40.0, 40.0), Color::rgb(0.8, 0.2, 0.2)),
        DrawCommand::PopElement,
        DrawCommand::PopElement,
    ]);
    commands
}

const CLEAR: Option<Color> = Some(Color::rgb(0.98, 0.98, 0.98));

/// Every node inside the host, in document order.
fn nodes_in(host: &web_sys::HtmlElement) -> Vec<web_sys::Node> {
    fn walk(node: &web_sys::Node, out: &mut Vec<web_sys::Node>) {
        let children = node.child_nodes();
        for index in 0..children.length() {
            if let Some(child) = children.item(index) {
                out.push(child.clone());
                walk(&child, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(host.as_ref(), &mut out);
    out
}

#[wasm_bindgen_test]
fn every_node_the_page_was_served_with_is_the_one_the_first_frame_keeps() {
    let commands = page(&WRITTEN);
    let host = serve(&commands, CLEAR);
    let served = nodes_in(&host);
    assert!(served.len() > 10, "{}", host.inner_html());

    take_over(&commands, CLEAR);

    let kept: Vec<web_sys::Node> = nodes_in(&host)
        .into_iter()
        .filter(|node| {
            !node
                .dyn_ref::<web_sys::Element>()
                .is_some_and(|element| element.has_attribute("data-telar-entry"))
        })
        .collect();
    assert_eq!(kept.len(), served.len(), "{}", host.inner_html());
    for (kept, served) in kept.iter().zip(&served) {
        assert!(same(kept, served), "{:?} was replaced", served.node_name());
    }
    assert_eq!(without_entry(&host), built_from_nothing(&commands, CLEAR));
}

/// Watches everything under the host but the host itself, and reports what changed as `kind target-name`.
struct Watch {
    observer: web_sys::MutationObserver,
    _callback: Closure<dyn FnMut()>,
}

impl Watch {
    fn start(host: &web_sys::HtmlElement) -> Self {
        let callback = Closure::<dyn FnMut()>::new(|| {});
        let observer =
            web_sys::MutationObserver::new(callback.as_ref().unchecked_ref()).expect("an observer");
        let options = web_sys::MutationObserverInit::new();
        options.set_child_list(true);
        options.set_subtree(true);
        options.set_attributes(true);
        options.set_character_data(true);
        observer
            .observe_with_options(host.as_ref(), &options)
            .expect("observing the host");
        Self {
            observer,
            _callback: callback,
        }
    }

    fn changes(self, host: &web_sys::HtmlElement) -> Vec<String> {
        let records = self.observer.take_records();
        self.observer.disconnect();
        let mut changes = Vec::new();
        for record in records.iter() {
            let record: web_sys::MutationRecord = record.unchecked_into();
            let Some(target) = record.target() else {
                continue;
            };
            let kind = record.type_();
            if kind == "childList" {
                let added = record.added_nodes();
                let only_the_entry = record.removed_nodes().length() == 0
                    && (0..added.length())
                        .filter_map(|i| added.item(i))
                        .all(|node| {
                            node.dyn_ref::<web_sys::Element>()
                                .is_some_and(|element| element.has_attribute("data-telar-entry"))
                        });
                if same(&target, host.as_ref()) && only_the_entry {
                    continue;
                }
            }
            if kind == "attributes" && same(&target, host.as_ref()) {
                continue;
            }
            changes.push(format!(
                "{kind} {} {}",
                target.node_name(),
                record.attribute_name().unwrap_or_default()
            ));
        }
        changes
    }
}

#[wasm_bindgen_test]
fn a_clean_take_over_changes_nothing_inside_the_host() {
    let commands = page(&WRITTEN);
    let host = serve(&commands, CLEAR);
    let watch = Watch::start(&host);
    take_over(&commands, CLEAR);
    assert_eq!(watch.changes(&host), Vec::<String>::new());
}

#[wasm_bindgen_test]
fn what_the_frame_says_differently_is_patched_where_it_stands() {
    let host = serve(&page(&WRITTEN), CLEAR);
    let heading = element(HEADING);
    let heading_text = heading.first_child().expect("the heading's text");
    let button = element(BUTTON);
    let main = element(MAIN);
    button
        .set_attribute("aria-hidden", "true")
        .expect("an attribute");

    let commands = page(&Variant {
        heading: "Hello & <welcome>",
        gap: "gap:16px;",
        button_label: "Change theme",
        ..WRITTEN
    });
    let watch = Watch::start(&host);
    take_over(&commands, CLEAR);
    let changes = watch.changes(&host);

    assert!(same(&element(HEADING), &heading));
    assert!(same(&element(BUTTON), &button));
    assert!(same(&element(MAIN), &main));
    assert!(
        same(
            &element(HEADING).first_child().expect("the heading's text"),
            &heading_text
        ),
        "the text is rewritten in its own node, so a selection inside it survives"
    );
    assert_eq!(heading.text_content().as_deref(), Some("Hello & <welcome>"));
    assert_eq!(
        button.get_attribute("aria-label").as_deref(),
        Some("Change theme")
    );
    assert_eq!(button.get_attribute("aria-hidden"), None);
    assert!(
        main.get_attribute("style")
            .unwrap_or_default()
            .contains("gap:16px"),
        "{:?}",
        main.get_attribute("style")
    );
    assert!(
        changes
            .iter()
            .all(|change| !change.starts_with("childList")),
        "{changes:?}"
    );
    assert_eq!(without_entry(&host), built_from_nothing(&commands, CLEAR));
}

#[wasm_bindgen_test]
fn a_box_served_as_another_element_is_rebuilt_alone() {
    let served_as = Variant {
        link: Role::Button,
        link_holds_stamp: true,
        ..WRITTEN
    };
    let host = serve(&page(&served_as), CLEAR);
    let served_link = element(LINK);
    let stamp = element(STAMP);
    let heading = element(HEADING);
    let button = element(BUTTON);
    let main = element(MAIN);
    assert_eq!(served_link.local_name(), "button");

    let commands = page(&WRITTEN);
    take_over(&commands, CLEAR);

    let link = element(LINK);
    assert_eq!(link.local_name(), "a");
    assert!(!same(&link, &served_link));
    assert!(!host.contains(Some(served_link.as_ref())));
    assert!(!host.contains(Some(stamp.as_ref())));
    assert!(same(&element(HEADING), &heading));
    assert!(same(&element(BUTTON), &button));
    assert!(same(&element(MAIN), &main));
    assert_eq!(without_entry(&host), built_from_nothing(&commands, CLEAR));
}

#[wasm_bindgen_test]
fn a_box_served_under_another_parent_is_rebuilt_where_it_belongs() {
    let served_as = Variant {
        link_holds_stamp: true,
        ..WRITTEN
    };
    let host = serve(&page(&served_as), CLEAR);
    let stamp = element(STAMP);
    let link = element(LINK);
    let mut commands = page(&WRITTEN);
    let after_button = commands
        .iter()
        .position(|command| matches!(command, DrawCommand::PushElement { element } if element.id.0 == PARAGRAPH))
        .expect("the paragraph opens");
    commands.splice(
        after_button..after_button,
        [
            boxed(
                STAMP,
                Semantics::group(),
                "width:10px;height:10px;",
                Rect::new(0.0, 120.0, 10.0, 10.0),
            ),
            DrawCommand::PopElement,
        ],
    );
    take_over(&commands, CLEAR);

    let moved = element(STAMP);
    assert!(!same(&moved, &stamp));
    assert!(same(
        &moved.parent_node().expect("in the document"),
        element(MAIN).as_ref()
    ));
    assert!(same(&element(LINK), &link));
    assert_eq!(without_entry(&host), built_from_nothing(&commands, CLEAR));
}

fn click(target: &web_sys::HtmlElement) -> bool {
    let init = web_sys::MouseEventInit::new();
    init.set_bubbles(true);
    init.set_cancelable(true);
    let event =
        web_sys::MouseEvent::new_with_mouse_event_init_dict("click", &init).expect("a click");
    let was_prevented = std::rc::Rc::new(std::cell::Cell::new(false));
    let seen = was_prevented.clone();
    let stop = Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
        seen.set(event.default_prevented());
        event.prevent_default();
    });
    document()
        .add_event_listener_with_callback("click", stop.as_ref().unchecked_ref())
        .expect("listening");
    target.dispatch_event(&event).expect("dispatched");
    document()
        .remove_event_listener_with_callback("click", stop.as_ref().unchecked_ref())
        .expect("no longer listening");
    was_prevented.get()
}

async fn next_frames() {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        let _ = window().set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 50);
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

fn recorded() -> Vec<Event> {
    RECORDED.with(|recorded| std::mem::take(&mut *recorded.borrow_mut()))
}

#[wasm_bindgen_test]
async fn a_link_and_the_keyboard_work_on_a_page_taken_over() {
    let commands = page(&WRITTEN);
    serve(&commands, CLEAR);
    let link: web_sys::HtmlElement = element(LINK).dyn_into().expect("an HTML element");
    let button: web_sys::HtmlElement = element(BUTTON).dyn_into().expect("an HTML element");
    take_over(&commands, CLEAR);
    next_frames().await;
    recorded();

    assert!(click(&link), "a plain click on a route link is the app's");
    next_frames().await;
    assert!(
        recorded()
            .iter()
            .any(|event| matches!(event, Event::BoxActivated { box_id } if *box_id == LINK))
    );

    button.focus().expect("the button takes focus");
    let init = web_sys::KeyboardEventInit::new();
    init.set_key("Enter");
    init.set_bubbles(true);
    init.set_cancelable(true);
    let enter =
        web_sys::KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &init).expect("a key");
    button.dispatch_event(&enter).expect("dispatched");
    assert!(
        enter.default_prevented(),
        "the button keeps the Enter it declared"
    );
    next_frames().await;
    let events = recorded();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::BoxFocused { box_id } if *box_id == BUTTON)),
        "{events:?}"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::KeyPressed {
                key: Key::Named(NamedKey::Enter),
                ..
            }
        )),
        "{events:?}"
    );
    button.blur().expect("the button lets go");
}

const PAGE: u64 = 20;
const TOP_LINK: u64 = 21;
const PANE: u64 = 22;
const PANE_CONTENT: u64 = 23;
const TAIL: u64 = 24;

/// A page that scrolls as the document, with a link at its top and a pane that scrolls itself.
fn long_page() -> Vec<DrawCommand> {
    vec![
        open(
            Element::new(
                ElementId(PAGE),
                Semantics::of(Role::ScrollArea),
                "display:flex;flex-direction:column;",
                Rect::new(0.0, 0.0, 400.0, 300.0),
            )
            .as_primary_scroll(true),
        ),
        boxed(
            TOP_LINK,
            Semantics::of(Role::Link)
                .linking_to(Destination::Route(Location::root().segment("arriba")))
                .focusable(Focusable {
                    tab_stop: true,
                    consumes: Role::Link.consumed_keys(),
                }),
            "height:20px;",
            Rect::new(0.0, 0.0, 400.0, 20.0),
        ),
        text("Arriba", Rect::new(0.0, 0.0, 400.0, 20.0)),
        DrawCommand::PopElement,
        boxed(
            PANE,
            Semantics::of(Role::ScrollArea),
            "height:100px;overflow:auto;",
            Rect::new(0.0, 20.0, 400.0, 100.0),
        ),
        boxed(
            PANE_CONTENT,
            Semantics::group(),
            "height:600px;",
            Rect::new(0.0, 20.0, 400.0, 600.0),
        ),
        DrawCommand::PopElement,
        DrawCommand::PopElement,
        boxed(
            TAIL,
            Semantics::of(Role::Article),
            "height:4000px;",
            Rect::new(0.0, 120.0, 400.0, 4000.0),
        ),
        text("Una página larga", Rect::new(0.0, 120.0, 400.0, 4000.0)),
        DrawCommand::PopElement,
        DrawCommand::PopElement,
    ]
}

#[wasm_bindgen_test]
async fn the_reader_s_scroll_and_focus_survive_the_take_over() {
    let commands = long_page();
    let host = serve(&commands, CLEAR);
    let link: web_sys::HtmlElement = element(TOP_LINK).dyn_into().expect("an HTML element");
    link.focus().expect("the link takes focus");
    window().scroll_to_with_x_and_y(0.0, 600.0);
    let pane = element(PANE);
    pane.set_scroll_top(50);
    assert_eq!(window().scroll_y().unwrap_or_default(), 600.0);
    assert_eq!(pane.scroll_top(), 50);
    next_frames().await;
    recorded();

    take_over(&commands, CLEAR);
    next_frames().await;

    assert_eq!(window().scroll_y().unwrap_or_default(), 600.0);
    assert_eq!(element(PANE).scroll_top(), 50);
    assert!(same(element(PANE).as_ref(), pane.as_ref()));
    let active = document().active_element().expect("something has focus");
    assert!(same(active.as_ref(), link.as_ref()));
    let events = recorded();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::BoxFocused { box_id } if *box_id == TOP_LINK)),
        "{events:?}"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::BoxScrolled { box_id, y, .. } if *box_id == PANE && *y == 50.0
        )),
        "{events:?}"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::BoxScrolled { box_id, y, .. } if *box_id == PAGE && *y == 600.0
        )),
        "{events:?}"
    );
    link.blur().expect("the link lets go");
    drop(host);
}

#[wasm_bindgen_test]
fn a_host_served_scrolling_as_the_document_is_given_back_without_the_overrides() {
    let commands = long_page();
    let host = serve(&commands, CLEAR);
    assert_eq!(
        host.style().get_property_value("height").ok().as_deref(),
        Some("auto")
    );
    take_over(&commands, CLEAR);
    SURFACE.with(|surface| {
        if let Some(surface) = surface.borrow_mut().as_mut() {
            surface.renderer = None;
        }
    });
    assert_eq!(
        host.style().get_property_value("height").ok().as_deref(),
        Some("")
    );
    assert!(!host.has_attribute("data-telar-document-scroll"));
}

#[wasm_bindgen_test]
async fn a_page_the_reader_scrolled_with_nothing_focused_stays_where_they_left_it() {
    let commands = long_page();
    serve(&commands, CLEAR);
    if let Some(active) = document().active_element() {
        let _ = active
            .dyn_into::<web_sys::HtmlElement>()
            .map(|active| active.blur());
    }
    window().scroll_to_with_x_and_y(0.0, 900.0);
    next_frames().await;

    take_over(&commands, CLEAR);
    next_frames().await;

    assert_eq!(window().scroll_y().unwrap_or_default(), 900.0);
}
