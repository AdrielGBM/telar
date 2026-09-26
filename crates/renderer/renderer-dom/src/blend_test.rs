//! `blend:<mode>` (`StyledContainer::with_blend`) in the document: `mix-blend-mode` on the box itself, and `isolation: isolate` on its parent so the blend does not reach past it to the page.
//!
//! Needs a browser, so it runs under `wasm-bindgen-test` rather than `cargo test` — see the crate README.

#![cfg(target_arch = "wasm32")]

use std::sync::Arc;

use geometry_core::Rect;
use renderer_core::{
    BlendMode, Color, DrawCommand, Element, ElementId, RectStyle, RenderBackend, Semantics,
    ShapeStyle,
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

fn host() -> web_sys::HtmlElement {
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

fn element(id: u64, rect: Rect) -> Arc<Element> {
    Arc::new(Element::new(ElementId(id), Semantics::group(), "", rect))
}

fn commands(blend: BlendMode) -> Vec<DrawCommand> {
    vec![
        DrawCommand::PushElement {
            element: element(1, Rect::new(0.0, 0.0, 100.0, 100.0)),
        },
        DrawCommand::PushElement {
            element: element(2, Rect::new(0.0, 0.0, 50.0, 50.0)),
        },
        DrawCommand::PushLayer {
            opacity: 1.0,
            backdrop_blur: 0.0,
            blend,
        },
        DrawCommand::Rect {
            rect: Rect::new(0.0, 0.0, 50.0, 50.0),
            style: Arc::new(RectStyle::default().with_fill(Color::WHITE)),
        },
        DrawCommand::PopLayer,
        DrawCommand::PopElement,
        DrawCommand::PopElement,
    ]
}

/// The plain case: `blend:normal` (the default) writes neither declaration, so an unblended box costs nothing extra.
#[wasm_bindgen_test]
fn normal_blend_writes_no_mix_blend_mode_or_isolation() {
    let host = host();
    let mut renderer = DomRenderer::new(host.clone()).expect("a renderer on the host");
    renderer
        .render_frame(&commands(BlendMode::Normal), None)
        .expect("the frame reconciled");

    let parent = host.first_element_child().expect("the outer box");
    let child = parent.first_element_child().expect("the blended box");

    let parent_style = parent.get_attribute("style").unwrap_or_default();
    let child_style = child.get_attribute("style").unwrap_or_default();
    assert!(
        !child_style.contains("mix-blend-mode"),
        "normal is the identity blend and should write nothing: {child_style}"
    );
    assert!(
        !parent_style.contains("isolation"),
        "a normal child asks nothing of its parent's stacking context: {parent_style}"
    );
    host.remove();
}

/// A real blend mode writes `mix-blend-mode` on the box itself and `isolation: isolate` on its parent, so the blend is confined to what that parent draws rather than reaching past it to the page.
#[wasm_bindgen_test]
fn multiply_writes_mix_blend_mode_and_isolates_the_parent() {
    let host = host();
    let mut renderer = DomRenderer::new(host.clone()).expect("a renderer on the host");
    renderer
        .render_frame(&commands(BlendMode::Multiply), None)
        .expect("the frame reconciled");

    let parent = host.first_element_child().expect("the outer box");
    let child = parent.first_element_child().expect("the blended box");

    let parent_style = parent.get_attribute("style").unwrap_or_default();
    let child_style = child.get_attribute("style").unwrap_or_default();
    assert!(
        child_style.contains("mix-blend-mode:multiply"),
        "the blended box carries its own mode: {child_style}"
    );
    assert!(
        parent_style.contains("isolation:isolate"),
        "the parent isolates so the blend stays confined to its own children: {parent_style}"
    );
    host.remove();
}

/// A blended box that is itself a layout root has no Telar parent — its backdrop is the page behind the host — so the host element takes the isolation instead.
#[wasm_bindgen_test]
fn a_blended_layout_root_isolates_the_host() {
    let host = host();
    let mut renderer = DomRenderer::new(host.clone()).expect("a renderer on the host");
    let commands = vec![
        DrawCommand::PushElement {
            element: element(1, Rect::new(0.0, 0.0, 50.0, 50.0)),
        },
        DrawCommand::PushLayer {
            opacity: 1.0,
            backdrop_blur: 0.0,
            blend: BlendMode::Screen,
        },
        DrawCommand::Rect {
            rect: Rect::new(0.0, 0.0, 50.0, 50.0),
            style: Arc::new(RectStyle::default().with_fill(Color::WHITE)),
        },
        DrawCommand::PopLayer,
        DrawCommand::PopElement,
    ];
    renderer
        .render_frame(&commands, None)
        .expect("the frame reconciled");

    let isolation = host
        .style()
        .get_property_value("isolation")
        .unwrap_or_default();
    assert_eq!(isolation, "isolate");
    host.remove();
}
