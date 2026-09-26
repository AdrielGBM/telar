//! What a document says about each box to a screen reader: the name, the language and what is hidden, as the attributes a browser builds its accessibility tree from. Needs a browser, so it runs under `wasm-bindgen-test` rather than `cargo test`.

#![cfg(target_arch = "wasm32")]

use std::sync::Arc;

use geometry_core::Rect;
use renderer_core::{
    Color, DrawCommand, Element, ElementId, RenderBackend, Role, Semantics, TextStyle,
};
use telar_renderer_dom::DomRenderer;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

fn document() -> web_sys::Document {
    web_sys::window()
        .and_then(|window| window.document())
        .expect("a document")
}

fn open(id: u64, semantics: Semantics) -> DrawCommand {
    DrawCommand::PushElement {
        element: Arc::new(Element::new(
            ElementId(id),
            semantics,
            "width:40px;height:20px",
            Rect::new(0.0, 0.0, 40.0, 20.0),
        )),
    }
}

fn text(content: &str) -> DrawCommand {
    DrawCommand::Text {
        text: Arc::from(content),
        spans: None,
        rect: Rect::new(0.0, 0.0, 10.0, 20.0),
        style: Arc::new(TextStyle::new(14.0, Color::BLACK)),
    }
}

/// Renders `commands` into a fresh host inside a root element, returning the root's single child: the box the frame opened first.
fn rendered(commands: &[DrawCommand]) -> web_sys::Element {
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
    let mut renderer = DomRenderer::new(host.clone()).expect("a renderer on the host");
    renderer
        .render_frame(commands, None)
        .expect("the frame reconciled");
    host.first_element_child().expect("the frame drew a box")
}

fn attribute(node: &web_sys::Element, name: &str) -> Option<String> {
    node.get_attribute(name)
}

/// Split letters: the row carries the word as its name and language, and each letter is kept from being read on its own.
#[wasm_bindgen_test]
fn split_letters_are_one_named_word_with_hidden_letters() {
    let row = rendered(&[
        open(1, Semantics::group().with_label("Hi").with_lang("en")),
        open(2, Semantics::group().hidden_from_readers()),
        text("H"),
        DrawCommand::PopElement,
        open(3, Semantics::group().hidden_from_readers()),
        text("i"),
        DrawCommand::PopElement,
        DrawCommand::PopElement,
    ]);
    assert_eq!(attribute(&row, "aria-label").as_deref(), Some("Hi"));
    assert_eq!(attribute(&row, "lang").as_deref(), Some("en"));
    assert_eq!(
        attribute(&row, "role").as_deref(),
        Some("group"),
        "a plain div may not carry a name, so the named box says it is a group"
    );
    assert_eq!(attribute(&row, "aria-hidden"), None);
    let letters = row.child_nodes();
    assert_eq!(letters.length(), 2);
    for i in 0..letters.length() {
        let letter: web_sys::Element = letters
            .item(i)
            .expect("a letter")
            .dyn_into()
            .expect("a letter is an element");
        assert_eq!(attribute(&letter, "aria-hidden").as_deref(), Some("true"));
    }
}

/// A picture nobody named is decoration; a named one is an image with that name.
#[wasm_bindgen_test]
fn a_picture_is_read_only_when_it_is_named() {
    let unnamed = rendered(&[open(10, Semantics::drawing()), DrawCommand::PopElement]);
    assert_eq!(attribute(&unnamed, "aria-hidden").as_deref(), Some("true"));
    assert_eq!(attribute(&unnamed, "aria-label"), None);

    let named = rendered(&[
        open(11, Semantics::drawing().with_label("Company logo")),
        DrawCommand::PopElement,
    ]);
    assert_eq!(named.tag_name().to_lowercase(), "svg");
    assert_eq!(attribute(&named, "role").as_deref(), Some("img"));
    assert_eq!(
        attribute(&named, "aria-label").as_deref(),
        Some("Company logo")
    );
    assert_eq!(attribute(&named, "aria-hidden"), None);
}

/// An element that is its role keeps it: a named heading is still an `<h1>`, and only gains the name.
#[wasm_bindgen_test]
fn a_named_heading_stays_a_heading() {
    let heading = rendered(&[
        open(20, Semantics::of(Role::Heading(1)).with_label("Welcome")),
        text("W"),
        DrawCommand::PopElement,
    ]);
    assert_eq!(heading.tag_name().to_lowercase(), "h1");
    assert_eq!(attribute(&heading, "role"), None);
    assert_eq!(
        attribute(&heading, "aria-label").as_deref(),
        Some("Welcome")
    );
}

/// Taking the annotation away takes the attribute with it, so a box that stops being hidden is read again.
#[wasm_bindgen_test]
fn an_annotation_withdrawn_is_an_attribute_removed() {
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
    let mut renderer = DomRenderer::new(host.clone()).expect("a renderer on the host");
    let frame = |semantics: Semantics| [open(30, semantics), text("x"), DrawCommand::PopElement];
    renderer
        .render_frame(
            &frame(Semantics::group().hidden_from_readers().with_lang("ja")),
            None,
        )
        .expect("the frame reconciled");
    let node = host.first_element_child().expect("a box");
    assert_eq!(attribute(&node, "aria-hidden").as_deref(), Some("true"));
    renderer
        .render_frame(&frame(Semantics::group()), None)
        .expect("the frame reconciled");
    let node = host.first_element_child().expect("a box");
    assert_eq!(attribute(&node, "aria-hidden"), None);
    assert_eq!(attribute(&node, "lang"), None);
}
