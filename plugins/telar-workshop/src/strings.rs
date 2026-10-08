//! The text the workshop draws itself, and the one way its chrome asks for it.

use telar::i18n::{Catalog, Entry, Message, Part, translate_with_override};

const NAMESPACE: &str = "telar_workshop";

pub(crate) const WORKSHOP: &str = "workshop";
pub(crate) const SEARCH: &str = "search";
pub(crate) const VIEW_CANVAS: &str = "view_canvas";
pub(crate) const VIEW_MATRIX: &str = "view_matrix";
pub(crate) const VIEW_DOCS: &str = "view_docs";
pub(crate) const PREVIEWS: &str = "previews";
pub(crate) const CLEAR_SEARCH: &str = "clear_search";
pub(crate) const NO_MATCHES: &str = "no_matches";
pub(crate) const NO_PREVIEWS: &str = "no_previews";
pub(crate) const PANELS: &str = "panels";
pub(crate) const CONTROLS: &str = "controls";
pub(crate) const BUILD_FAILED: &str = "build_failed";
pub(crate) const NO_PREVIEWS_HINT: &str = "no_previews_hint";
pub(crate) const OPEN_IN_EDITOR: &str = "open_in_editor";
pub(crate) const PANICKED: &str = "panicked";
pub(crate) const REMOUNT: &str = "remount";
pub(crate) const VIEWPORT_HEIGHT: &str = "viewport_height";
pub(crate) const VIEWPORT_RESET: &str = "viewport_reset";
pub(crate) const VIEWPORT_SIZE: &str = "viewport_size";
pub(crate) const VIEWPORT_WIDTH: &str = "viewport_width";
pub(crate) const COLUMN_DEFAULT: &str = "column_default";
pub(crate) const COLUMN_DESCRIPTION: &str = "column_description";
pub(crate) const COLUMN_NAME: &str = "column_name";
pub(crate) const COLUMN_VALUE: &str = "column_value";
pub(crate) const NO_ARGS: &str = "no_args";
pub(crate) const NOTHING_SELECTED: &str = "nothing_selected";
pub(crate) const RESET: &str = "reset";
pub(crate) const RESET_ALL: &str = "reset_all";
pub(crate) const RESET_ARG: &str = "reset_arg";
pub(crate) const UNSET: &str = "unset";
pub(crate) const UNSET_ARG: &str = "unset_arg";
pub(crate) const CANVAS_TOOLS: &str = "canvas_tools";
pub(crate) const RESIZE_PANELS: &str = "resize_panels";
pub(crate) const RESIZE_SIDEBAR: &str = "resize_sidebar";

macro_rules! english {
    ($key:expr, $text:literal) => {
        Entry {
            key: $key,
            messages: &[("en", Message::Plain($text))],
        }
    };
}

macro_rules! with_arg {
    ($key:expr, $prefix:literal) => {
        Entry {
            key: $key,
            messages: &[(
                "en",
                Message::Format(&[Part::Lit($prefix), Part::Arg("arg")]),
            )],
        }
    };
}

static CATALOG: Catalog = Catalog {
    locales: &["en"],
    default_locale: "en",
    // Sorted by key: a lookup binary-searches them.
    entries: &[
        english!(BUILD_FAILED, "This preview failed to build"),
        english!(CANVAS_TOOLS, "Canvas tools"),
        english!(CLEAR_SEARCH, "Clear search"),
        english!(COLUMN_DEFAULT, "Default"),
        english!(COLUMN_DESCRIPTION, "Description"),
        english!(COLUMN_NAME, "Name"),
        english!(COLUMN_VALUE, "Value"),
        english!(CONTROLS, "Controls"),
        english!(NO_ARGS, "No args"),
        english!(NO_MATCHES, "No matches"),
        english!(NO_PREVIEWS, "No previews"),
        english!(
            NO_PREVIEWS_HINT,
            "Write a [preview] block in an .rsx file, or a preview! in Rust, and it is listed here."
        ),
        english!(NOTHING_SELECTED, "Nothing selected"),
        english!(OPEN_IN_EDITOR, "Open in editor"),
        english!(PANELS, "Panels"),
        english!(PANICKED, "This preview panicked"),
        english!(PREVIEWS, "Previews"),
        english!(REMOUNT, "Remount"),
        english!(RESET, "Reset"),
        english!(RESET_ALL, "Reset all"),
        with_arg!(RESET_ARG, "Reset "),
        english!(RESIZE_PANELS, "Resize the panels"),
        english!(RESIZE_SIDEBAR, "Resize the sidebar"),
        english!(SEARCH, "Search previews"),
        english!(UNSET, "Unset"),
        with_arg!(UNSET_ARG, "Unset "),
        english!(VIEW_CANVAS, "Canvas"),
        english!(VIEW_DOCS, "Docs"),
        english!(VIEW_MATRIX, "Matrix"),
        english!(VIEWPORT_HEIGHT, "Viewport height"),
        english!(VIEWPORT_RESET, "Reset the viewport"),
        english!(VIEWPORT_SIZE, "Viewport size"),
        english!(VIEWPORT_WIDTH, "Viewport width"),
        english!(WORKSHOP, "Workshop"),
    ],
};

/// Resolves `key` for the active locale, preferring the application's `telar_workshop.<key>` message over the English one shipped here. Call it inside a reactive closure so a locale switch re-renders.
pub(crate) fn text(key: &str) -> String {
    translate_with_override(NAMESPACE, &CATALOG, key, &[])
}

/// [`text`] for a message naming one arg of a preview, as `{arg}`.
pub(crate) fn text_naming(key: &str, arg: &str) -> String {
    translate_with_override(NAMESPACE, &CATALOG, key, &[("arg", arg)])
}

#[cfg(test)]
#[path = "strings_test.rs"]
mod tests;
