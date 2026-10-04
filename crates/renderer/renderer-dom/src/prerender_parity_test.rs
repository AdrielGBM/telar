//! A prerendered page is the page the reconcile builds: the same frame, written out as markup on a host and reconciled into a live document in the browser, comes out as the same elements with the same attributes. What lets a client adopt the page it was served instead of building it again. Needs a browser, so it runs under `wasm-bindgen-test` rather than `cargo test`.

#![cfg(target_arch = "wasm32")]

use std::sync::Arc;

use geometry_core::Rect;
use platform_core::{Destination, Location};
use renderer_core::{
    Color, ConsumedKeys, Declared, DrawCommand, Element, ElementId, Focusable, ImageData, Picture,
    RectStyle, RenderBackend, Role, Semantics, Span, TextStyle,
};
use telar_renderer_dom::{DomRenderer, ID_ATTRIBUTE, prerender};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

/// The host properties either writer may set, compared by what the browser reads them as rather than how each spelled them.
const HOST_PROPERTIES: [&str; 7] = [
    "position",
    "background-color",
    "color-scheme",
    "isolation",
    "height",
    "touch-action",
    "overflow-x",
];

fn document() -> web_sys::Document {
    web_sys::window()
        .and_then(|window| window.document())
        .expect("a document")
}

fn new_host() -> web_sys::HtmlElement {
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
    host
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

/// The content the reconcile left in `host`, without the one editable element it keeps there for typing, which a page is served without.
fn reconciled_markup(host: &web_sys::HtmlElement) -> String {
    let copy: web_sys::Element = host
        .clone_node_with_deep(true)
        .expect("the host clones")
        .dyn_into()
        .expect("a clone of an element is one");
    let entries = copy
        .query_selector_all("textarea")
        .expect("a selector the browser reads");
    for index in 0..entries.length() {
        if let Some(entry) = entries.item(index) {
            let _ = copy.remove_child(&entry);
        }
    }
    copy.inner_html()
}

/// Reconciles `commands` into one host and writes them as a page into another, and checks the browser reads the two as the same document.
fn assert_same_document(commands: &[DrawCommand], clear: Option<Color>) {
    let live = new_host();
    let mut renderer = DomRenderer::new(live.clone()).expect("a renderer on the host");
    renderer
        .render_frame(commands, clear)
        .expect("the frame reconciled");

    let written = prerender(commands, clear);
    let served = new_host();
    for (name, value) in &written.host_attributes {
        served
            .set_attribute(name, value)
            .expect("a host attribute the browser takes");
    }
    served.set_inner_html(&written.markup);

    assert_eq!(reconciled_markup(&live), served.inner_html());
    for property in HOST_PROPERTIES {
        assert_eq!(
            live.style().get_property_value(property).ok(),
            served.style().get_property_value(property).ok(),
            "the host's {property}"
        );
    }
    for attribute in ["data-telar", "data-telar-document-scroll"] {
        assert_eq!(
            live.has_attribute(attribute),
            served.has_attribute(attribute),
            "the host's {attribute}"
        );
    }
    drop(renderer);
    live.remove();
    served.remove();
}

fn hero() -> ImageData {
    ImageData::linked(
        "images/hero.png",
        1200,
        800,
        &[(600, "images/hero-600w.png")],
    )
}

fn page() -> Vec<DrawCommand> {
    let full = Rect::new(0.0, 0.0, 640.0, 480.0);
    let picture = Picture {
        source: hero().linked_source().expect("a linked picture").clone(),
        width: 1200,
        height: 800,
        fit: "cover",
        priority: true,
    };
    let paragraph = Rect::new(0.0, 120.0, 640.0, 20.0);
    vec![
        fill(Rect::new(0.0, 0.0, 640.0, 40.0), Color::rgb(0.9, 0.9, 0.95)),
        boxed(
            1,
            Semantics::of(Role::Main).with_lang("es"),
            "display:flex;flex-direction:column;gap:8px;",
            full,
        ),
        boxed(
            2,
            Semantics::of(Role::Heading(1)).with_anchor("inicio"),
            "",
            Rect::new(0.0, 0.0, 640.0, 40.0),
        ),
        text("Hola & <bienvenidos>", Rect::new(0.0, 0.0, 640.0, 40.0)),
        DrawCommand::PopElement,
        boxed(
            3,
            Semantics::of(Role::Link).linking_to(Destination::Route(
                Location::root().segment("proyectos").with_locale("es"),
            )),
            "",
            Rect::new(0.0, 48.0, 120.0, 20.0),
        ),
        text("Proyectos", Rect::new(0.0, 48.0, 120.0, 20.0)),
        DrawCommand::PopElement,
        boxed(
            4,
            Semantics::of(Role::Button)
                .with_label("Cambiar tema")
                .focusable(Focusable {
                    tab_stop: true,
                    consumes: ConsumedKeys::default(),
                }),
            "width:40px;height:40px;",
            Rect::new(0.0, 76.0, 40.0, 40.0),
        ),
        fill(Rect::new(0.0, 76.0, 40.0, 40.0), Color::rgb(0.2, 0.3, 0.8)),
        fill(Rect::new(10.0, 86.0, 20.0, 20.0), Color::WHITE),
        DrawCommand::PopElement,
        boxed(5, Semantics::group(), "", paragraph),
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
        open(
            Element::new(
                ElementId(6),
                Semantics::group().with_label("Telar"),
                "width:300px;height:200px;",
                Rect::new(0.0, 148.0, 300.0, 200.0),
            )
            .showing(picture),
        ),
        DrawCommand::PopElement,
        boxed(
            7,
            Semantics::drawing().with_label("Un círculo"),
            "width:40px;height:40px;",
            Rect::new(0.0, 356.0, 40.0, 40.0),
        ),
        fill(Rect::new(0.0, 356.0, 40.0, 40.0), Color::rgb(0.8, 0.2, 0.2)),
        DrawCommand::PopElement,
        DrawCommand::PopElement,
    ]
}

#[wasm_bindgen_test]
fn a_page_written_ahead_is_the_page_the_reconcile_builds() {
    assert_same_document(&page(), Some(Color::rgb(0.98, 0.98, 0.98)));
}

#[wasm_bindgen_test]
fn a_page_that_scrolls_as_the_document_is_written_as_one() {
    let root = Element::new(
        ElementId(10),
        Semantics::of(Role::ScrollArea),
        "display:flex;flex-direction:column;",
        Rect::new(0.0, 0.0, 400.0, 300.0),
    )
    .as_primary_scroll(true);
    let commands = [
        open(root),
        boxed(
            11,
            Semantics::of(Role::Article),
            "height:900px;",
            Rect::new(0.0, 0.0, 400.0, 900.0),
        ),
        text("Una página larga", Rect::new(0.0, 0.0, 400.0, 900.0)),
        DrawCommand::PopElement,
        DrawCommand::PopElement,
        fill(Rect::new(0.0, 260.0, 400.0, 40.0), Color::BLACK),
    ];
    // A page is written for a host at the top left of a document nobody has scrolled yet, which is where the reconciled one has to stand too.
    let body = document().body().expect("a body");
    let _ = body.style().set_property("margin", "0");
    assert_same_document(&commands, Some(Color::rgb(0.05, 0.05, 0.1)));
    let _ = body.style().remove_property("margin");
}

#[wasm_bindgen_test]
fn every_box_names_the_box_it_stands_for() {
    let host = new_host();
    let mut renderer = DomRenderer::new(host.clone()).expect("a renderer on the host");
    renderer
        .render_frame(&page(), None)
        .expect("the frame reconciled");
    let named = host
        .query_selector_all(&format!("[{ID_ATTRIBUTE}]"))
        .expect("a selector the browser reads");
    let mut ids: Vec<String> = (0..named.length())
        .filter_map(|index| named.item(index))
        .filter_map(|node| node.dyn_into::<web_sys::Element>().ok())
        .filter_map(|element| element.get_attribute(ID_ATTRIBUTE))
        .collect();
    ids.sort();
    assert_eq!(ids, ["1", "2", "3", "4", "5", "6", "7"]);
    drop(renderer);
    host.remove();
}

#[wasm_bindgen_test]
fn the_page_a_host_was_served_with_is_taken_over_by_the_first_frame() {
    let commands = page();
    let reference = new_host();
    let mut renderer = DomRenderer::new(reference.clone()).expect("a renderer on the host");
    renderer
        .render_frame(&commands, None)
        .expect("the frame reconciled");
    let expected = reconciled_markup(&reference);
    drop(renderer);
    reference.remove();

    let served = new_host();
    served.set_inner_html(&prerender(&commands, None).markup);
    let first_served = served.first_child().expect("the served page has content");
    let mut renderer = DomRenderer::new(served.clone()).expect("a renderer on the host");
    renderer
        .render_frame(&commands, None)
        .expect("the frame reconciled");
    assert!(
        served.contains(Some(&first_served)),
        "what the page was served with is kept once a frame takes it over"
    );
    assert_eq!(reconciled_markup(&served), expected);
    drop(renderer);
    served.remove();
}
