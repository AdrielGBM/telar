//! A linked picture in a document: an `<img>` the browser fetches and sizes itself, and an address inside a drawing. Needs a browser, so it runs under `wasm-bindgen-test` rather than `cargo test`.

#![cfg(target_arch = "wasm32")]

use std::sync::Arc;

use geometry_core::Rect;
use renderer_core::{
    DrawCommand, Element, ElementId, ImageData, ImageFill, Picture, Raster, RenderBackend,
    Semantics,
};
use telar_renderer_dom::DomRenderer;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

const BASE: &str = "https://cdn.example/site/";

fn hero() -> ImageData {
    ImageData::linked("images/abc.png", 1200, 800, &[(600, "images/abc-600w.png")])
}

fn picture(priority: bool) -> Picture {
    Picture {
        source: hero().linked_source().expect("a linked picture").clone(),
        width: 1200,
        height: 800,
        fit: "cover",
        priority,
    }
}

fn frame(semantics: Semantics, picture: Option<Picture>) -> Vec<DrawCommand> {
    let rect = Rect::new(0.0, 0.0, 300.0, 200.0);
    let element = Element::new(ElementId(1), semantics, "width:300px;height:200px", rect);
    let element = match picture {
        Some(picture) => element.showing(picture),
        None => element,
    };
    vec![
        DrawCommand::PushElement {
            element: Arc::new(element),
        },
        DrawCommand::Image {
            data: Arc::new(hero()),
            rect,
            raster: Raster::Smooth,
            fill: ImageFill::Tile { scale: 1.0 },
        },
        DrawCommand::PopElement,
    ]
}

fn rendered(commands: &[DrawCommand]) -> web_sys::Element {
    platform_core::set_asset_base(BASE);
    let document = web_sys::window()
        .and_then(|window| window.document())
        .expect("a document");
    let host: web_sys::HtmlElement = document
        .create_element("div")
        .expect("a host element")
        .dyn_into()
        .expect("a div is an HTML element");
    document
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

#[wasm_bindgen_test]
fn a_picture_is_an_img_that_offers_its_smaller_copies() {
    let img = rendered(&frame(Semantics::drawing(), Some(picture(false))));
    let attribute = |name: &str| img.get_attribute(name);
    assert_eq!(img.tag_name(), "IMG");
    assert_eq!(
        attribute("src").as_deref(),
        Some("https://cdn.example/site/images/abc.png")
    );
    assert_eq!(
        attribute("srcset").as_deref(),
        Some(
            "https://cdn.example/site/images/abc-600w.png 600w, https://cdn.example/site/images/abc.png 1200w"
        )
    );
    assert_eq!(attribute("sizes").as_deref(), Some("300px"));
    assert_eq!(attribute("width").as_deref(), Some("1200"));
    assert_eq!(attribute("height").as_deref(), Some("800"));
    assert_eq!(attribute("loading").as_deref(), Some("lazy"));
    assert_eq!(attribute("fetchpriority"), None);
    assert_eq!(
        attribute("alt").as_deref(),
        Some(""),
        "unnamed is decoration"
    );
    assert_eq!(attribute("aria-hidden"), None);
    assert!(
        attribute("style").is_some_and(|style| style.contains("object-fit:cover")),
        "{:?}",
        attribute("style")
    );
    assert_eq!(img.child_element_count(), 0, "the browser draws it");
}

#[wasm_bindgen_test]
fn a_named_priority_picture_is_fetched_at_once() {
    let img = rendered(&frame(
        Semantics::drawing().with_label("A loom"),
        Some(picture(true)),
    ));
    assert_eq!(img.get_attribute("alt").as_deref(), Some("A loom"));
    assert_eq!(img.get_attribute("aria-label"), None);
    assert_eq!(img.get_attribute("loading"), None);
    assert_eq!(img.get_attribute("fetchpriority").as_deref(), Some("high"));
}

/// A fill CSS has no `object-fit` for stays a drawing, and draws from the address.
#[wasm_bindgen_test]
fn a_linked_picture_in_a_drawing_is_drawn_from_its_address() {
    let svg = rendered(&frame(Semantics::drawing().with_label("Tiles"), None));
    assert_eq!(svg.tag_name(), "svg");
    assert!(
        svg.inner_html()
            .contains("https://cdn.example/site/images/abc.png"),
        "{}",
        svg.inner_html()
    );
}
