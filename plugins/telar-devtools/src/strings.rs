//! The text the overlay draws, and the one way it asks for it.

use telar::i18n::{Catalog, english, translate_with_override};

const NAMESPACE: &str = "telar_devtools";

pub(crate) const BORDER: &str = "border";
pub(crate) const BUILD_ERROR: &str = "build_error";
pub(crate) const BUILD_FAILED: &str = "build_failed";
pub(crate) const BUILD_FAILED_HINT: &str = "build_failed_hint";
pub(crate) const CLOSE: &str = "close";
pub(crate) const COMPONENTS: &str = "components";
pub(crate) const DEPTH: &str = "depth";
pub(crate) const DETAILS: &str = "details";
pub(crate) const DEV: &str = "dev";
pub(crate) const DEVTOOLS: &str = "devtools";
pub(crate) const FPS: &str = "fps";
pub(crate) const FRAME_TIME: &str = "frame_time";
pub(crate) const GAP: &str = "gap";
pub(crate) const INSPECTOR: &str = "inspector";
pub(crate) const MARGIN: &str = "margin";
pub(crate) const NODES: &str = "nodes";
pub(crate) const NOTHING_SELECTED: &str = "nothing_selected";
pub(crate) const PADDING: &str = "padding";
pub(crate) const POSITION: &str = "position";
pub(crate) const RENDERER: &str = "renderer";
pub(crate) const RENDERER_STARTING: &str = "renderer_starting";
pub(crate) const RESIZE_DETAILS: &str = "resize_details";
pub(crate) const RESIZE_INSPECTOR: &str = "resize_inspector";
pub(crate) const SHOW_INSPECTOR: &str = "show_inspector";
pub(crate) const SHOW_PANEL: &str = "show_panel";
pub(crate) const SIZE: &str = "size";
pub(crate) const SWITCH_RENDERER: &str = "switch_renderer";
pub(crate) const TOGGLE_PANEL: &str = "toggle_panel";

static CATALOG: Catalog = Catalog {
    locales: &["en"],
    default_locale: "en",
    // Sorted by key: a lookup binary-searches them.
    entries: &[
        english!(BORDER, "Border"),
        english!(BUILD_ERROR, "Build error"),
        english!(BUILD_FAILED, "Build failed"),
        english!(
            BUILD_FAILED_HINT,
            "The app keeps running the last build that compiled."
        ),
        english!(CLOSE, "Close"),
        english!(COMPONENTS, "Components"),
        english!(DEPTH, "Depth"),
        english!(DETAILS, "Details"),
        english!(DEV, "DEV"),
        english!(DEVTOOLS, "Telar devtools"),
        english!(FPS, { arg }, " fps"),
        english!(FRAME_TIME, { arg }, " ms per frame"),
        english!(GAP, "Gap"),
        english!(INSPECTOR, "Inspector"),
        english!(MARGIN, "Margin"),
        english!(NODES, { arg }, " components"),
        english!(NOTHING_SELECTED, "Select a component to see its box."),
        english!(PADDING, "Padding"),
        english!(POSITION, "Position"),
        english!(RENDERER, "Renderer"),
        english!(RENDERER_STARTING, "Starting"),
        english!(RESIZE_DETAILS, "Resize the details"),
        english!(RESIZE_INSPECTOR, "Resize the inspector"),
        english!(SHOW_INSPECTOR, "Inspector"),
        english!(SHOW_PANEL, "This panel"),
        english!(SIZE, "Size"),
        english!(SWITCH_RENDERER, "Switch renderer"),
        english!(TOGGLE_PANEL, "Devtools panel"),
    ],
};

/// Resolves `key` for the active locale, preferring the application's `telar_devtools.<key>` message over the English one shipped here. Call it inside a reactive closure so a locale switch re-renders.
pub(crate) fn text(key: &str) -> String {
    translate_with_override(NAMESPACE, &CATALOG, key, &[])
}

/// [`text`] for a message with one value in it, as `{arg}`: a count or a duration.
pub(crate) fn text_with(key: &str, arg: &str) -> String {
    translate_with_override(NAMESPACE, &CATALOG, key, &[("arg", arg)])
}

#[cfg(test)]
#[path = "strings_test.rs"]
mod tests;
