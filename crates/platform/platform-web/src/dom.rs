//! The handful of browser globals everything here starts from.
//!
//! Each is fetched rather than held: they are process-wide singletons the browser guarantees, and a stored
//! handle only adds a lifetime to reason about.

use wasm_bindgen::JsCast;

pub fn window() -> web_sys::Window {
    web_sys::window().expect("a browser window")
}

pub fn document() -> web_sys::Document {
    window().document().expect("a document")
}

/// The id the generated page gives the element it means the app to fill.
const DEFAULT_HOST: &str = "telar-root";

/// The element the app fills, from a CSS selector.
///
/// With no selector: the element the page set aside, and `<body>` where there is none — an app that says
/// nothing wants the page to *be* the app. Preferring the element matters more than it looks: a backend that
/// writes the document owns everything inside its host, so mounting on `<body>` by default would clear the
/// page's own markup out from under it. A selector that matches nothing is an error rather than a silent
/// fallback — it is a typo in the app's own markup, and quietly mounting somewhere else is the kind of thing
/// nobody notices until the layout is wrong.
pub fn host(selector: Option<&str>) -> Result<web_sys::HtmlElement, String> {
    let document = document();
    match selector {
        Some(selector) => document
            .query_selector(selector)
            .map_err(|_| format!("`{selector}` is not a valid CSS selector"))?
            .ok_or_else(|| format!("no element matches `{selector}`"))?
            .dyn_into::<web_sys::HtmlElement>()
            .map_err(|_| format!("`{selector}` matched something that is not an HTML element")),
        None => document
            .get_element_by_id(DEFAULT_HOST)
            .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
            .or_else(|| document.body())
            .ok_or_else(|| "the document has no body".to_string()),
    }
}

/// Whether the user's system asks for a dark interface.
pub fn prefers_dark() -> Option<bool> {
    window()
        .match_media("(prefers-color-scheme: dark)")
        .ok()
        .flatten()
        .map(|query| query.matches())
}
