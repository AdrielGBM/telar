//! The text the workshop draws itself, and the one way its chrome asks for it.

use telar::Reactive;
use telar::i18n::{Catalog, english, translate_with_override};

const NAMESPACE: &str = "telar_workshop";

pub(crate) const ACTIONS: &str = "actions";
pub(crate) const BACKGROUND: &str = "background";
pub(crate) const BACKGROUND_DARK: &str = "background_dark";
pub(crate) const BACKGROUND_LIGHT: &str = "background_light";
pub(crate) const BACKGROUND_PREVIEW: &str = "background_preview";
pub(crate) const BACKGROUND_TRANSPARENT: &str = "background_transparent";
pub(crate) const BUILD_FAILED: &str = "build_failed";
pub(crate) const CANVAS_SETTINGS: &str = "canvas_settings";
pub(crate) const CANVAS_TOOLS: &str = "canvas_tools";
pub(crate) const CLEAR_ACTIONS: &str = "clear_actions";
pub(crate) const CLEAR_SEARCH: &str = "clear_search";
pub(crate) const COLUMN_DEFAULT: &str = "column_default";
pub(crate) const COLUMN_DESCRIPTION: &str = "column_description";
pub(crate) const COLUMN_NAME: &str = "column_name";
pub(crate) const COLUMN_TYPE: &str = "column_type";
pub(crate) const COLUMN_VALUE: &str = "column_value";
pub(crate) const CONTROL_SIZE: &str = "control_size";
pub(crate) const CONTROL_SIZE_DEFAULT: &str = "control_size_default";
pub(crate) const CONTROL_SIZE_LARGE: &str = "control_size_large";
pub(crate) const CONTROL_SIZE_MINI: &str = "control_size_mini";
pub(crate) const CONTROL_SIZE_REGULAR: &str = "control_size_regular";
pub(crate) const CONTROL_SIZE_SMALL: &str = "control_size_small";
pub(crate) const CONTROLS: &str = "controls";
pub(crate) const COPY_LINK: &str = "copy_link";
pub(crate) const CYCLE_DIRECTION: &str = "cycle_direction";
pub(crate) const CYCLE_LOCALE: &str = "cycle_locale";
pub(crate) const CYCLE_MODE: &str = "cycle_mode";
pub(crate) const DEVICE_COMPACT_PHONE: &str = "device_compact_phone";
pub(crate) const DEVICE_FRAMES: &str = "device_frames";
pub(crate) const DEVICE_PHONE: &str = "device_phone";
pub(crate) const DEVICE_TABLET: &str = "device_tablet";
pub(crate) const DIRECTION: &str = "direction";
pub(crate) const DIRECTION_AUTO: &str = "direction_auto";
pub(crate) const DIRECTION_LTR: &str = "direction_ltr";
pub(crate) const DIRECTION_RTL: &str = "direction_rtl";
pub(crate) const FILTER_ACTIONS: &str = "filter_actions";
pub(crate) const FIND: &str = "find";
pub(crate) const FULLSCREEN: &str = "fullscreen";
pub(crate) const GRID: &str = "grid";
pub(crate) const GROUP_CANVAS: &str = "group_canvas";
pub(crate) const GROUP_NAVIGATE: &str = "group_navigate";
pub(crate) const GROUP_VIEW: &str = "group_view";
pub(crate) const HIGH_CONTRAST: &str = "high_contrast";
pub(crate) const KEYBOARD_SHORTCUTS: &str = "keyboard_shortcuts";
pub(crate) const LOCALE: &str = "locale";
pub(crate) const LOCALE_DEFAULT: &str = "locale_default";
pub(crate) const MODE: &str = "mode";
pub(crate) const MODE_DEFAULT: &str = "mode_default";
pub(crate) const MOVE_PANELS: &str = "move_panels";
pub(crate) const NEXT_COMPONENT: &str = "next_component";
pub(crate) const NEXT_PREVIEW: &str = "next_preview";
pub(crate) const NO_ACTIONS: &str = "no_actions";
pub(crate) const NO_ARGS: &str = "no_args";
pub(crate) const NO_MATCHES: &str = "no_matches";
pub(crate) const NO_MATCHING_ACTIONS: &str = "no_matching_actions";
pub(crate) const NO_PREVIEWS: &str = "no_previews";
pub(crate) const NO_PREVIEWS_HINT: &str = "no_previews_hint";
pub(crate) const NOTHING_SELECTED: &str = "nothing_selected";
pub(crate) const OPEN_IN_CANVAS: &str = "open_in_canvas";
pub(crate) const OPEN_IN_EDITOR: &str = "open_in_editor";
pub(crate) const PANELS: &str = "panels";
pub(crate) const PANICKED: &str = "panicked";
pub(crate) const PRESET_DESKTOP: &str = "preset_desktop";
pub(crate) const PRESET_LAPTOP: &str = "preset_laptop";
pub(crate) const PRESET_MOBILE: &str = "preset_mobile";
pub(crate) const PRESET_TABLET: &str = "preset_tablet";
pub(crate) const PREVIEWS: &str = "previews";
pub(crate) const PREVIOUS_COMPONENT: &str = "previous_component";
pub(crate) const PREVIOUS_PREVIEW: &str = "previous_preview";
pub(crate) const PROJECT_VIEWPORTS: &str = "project_viewports";
pub(crate) const PROPS: &str = "props";
pub(crate) const REDUCED_MOTION: &str = "reduced_motion";
pub(crate) const REMOUNT: &str = "remount";
pub(crate) const REQUIRED: &str = "required";
pub(crate) const RESET: &str = "reset";
pub(crate) const RESET_ALL: &str = "reset_all";
pub(crate) const RESET_ARG: &str = "reset_arg";
pub(crate) const RESIZE_PANELS: &str = "resize_panels";
pub(crate) const RESIZE_SIDEBAR: &str = "resize_sidebar";
pub(crate) const ROTATE: &str = "rotate";
pub(crate) const RULERS: &str = "rulers";
pub(crate) const SEARCH: &str = "search";
pub(crate) const SHOW_CANVAS: &str = "show_canvas";
pub(crate) const SHOW_DOCS: &str = "show_docs";
pub(crate) const SOURCE_OF: &str = "source_of";
pub(crate) const TOGGLE_GRID: &str = "toggle_grid";
pub(crate) const TOGGLE_PANELS: &str = "toggle_panels";
pub(crate) const TOGGLE_RULERS: &str = "toggle_rulers";
pub(crate) const TOGGLE_SIDEBAR: &str = "toggle_sidebar";
pub(crate) const UNSET: &str = "unset";
pub(crate) const UNSET_ARG: &str = "unset_arg";
pub(crate) const VIEW: &str = "view";
pub(crate) const VIEW_CANVAS: &str = "view_canvas";
pub(crate) const VIEW_DOCS: &str = "view_docs";
pub(crate) const VIEWPORT: &str = "viewport";
pub(crate) const VIEWPORT_CUSTOM: &str = "viewport_custom";
pub(crate) const VIEWPORT_FILL: &str = "viewport_fill";
pub(crate) const VIEWPORT_HEIGHT: &str = "viewport_height";
pub(crate) const VIEWPORT_PRESETS: &str = "viewport_presets";
pub(crate) const VIEWPORT_PREVIEW: &str = "viewport_preview";
pub(crate) const VIEWPORT_RESET: &str = "viewport_reset";
pub(crate) const VIEWPORT_SIZE: &str = "viewport_size";
pub(crate) const VIEWPORT_WIDTH: &str = "viewport_width";
pub(crate) const WORKSHOP: &str = "workshop";
pub(crate) const ZOOM: &str = "zoom";
pub(crate) const ZOOM_FIT: &str = "zoom_fit";
pub(crate) const ZOOM_IN: &str = "zoom_in";
pub(crate) const ZOOM_OUT: &str = "zoom_out";
pub(crate) const ZOOM_PERCENT: &str = "zoom_percent";
pub(crate) const ZOOM_RESET: &str = "zoom_reset";

static CATALOG: Catalog = Catalog {
    locales: &["en"],
    default_locale: "en",
    // Sorted by key: a lookup binary-searches them.
    entries: &[
        english!(ACTIONS, "Actions"),
        english!(BACKGROUND, "Background"),
        english!(BACKGROUND_DARK, "Dark"),
        english!(BACKGROUND_LIGHT, "Light"),
        english!(BACKGROUND_PREVIEW, "Preview background"),
        english!(BACKGROUND_TRANSPARENT, "Transparent"),
        english!(BUILD_FAILED, "This preview failed to build"),
        english!(CANVAS_SETTINGS, "Canvas settings"),
        english!(CANVAS_TOOLS, "Canvas tools"),
        english!(CLEAR_ACTIONS, "Clear"),
        english!(CLEAR_SEARCH, "Clear search"),
        english!(COLUMN_DEFAULT, "Default"),
        english!(COLUMN_DESCRIPTION, "Description"),
        english!(COLUMN_NAME, "Name"),
        english!(COLUMN_TYPE, "Type"),
        english!(COLUMN_VALUE, "Value"),
        english!(CONTROL_SIZE, "Control size"),
        english!(CONTROL_SIZE_DEFAULT, "Default size"),
        english!(CONTROL_SIZE_LARGE, "Large"),
        english!(CONTROL_SIZE_MINI, "Mini"),
        english!(CONTROL_SIZE_REGULAR, "Regular"),
        english!(CONTROL_SIZE_SMALL, "Small"),
        english!(CONTROLS, "Controls"),
        english!(COPY_LINK, "Copy link"),
        english!(CYCLE_DIRECTION, "Next direction"),
        english!(CYCLE_LOCALE, "Next locale"),
        english!(CYCLE_MODE, "Next mode"),
        english!(DEVICE_COMPACT_PHONE, "Compact phone"),
        english!(DEVICE_FRAMES, "Devices"),
        english!(DEVICE_PHONE, "Phone"),
        english!(DEVICE_TABLET, "Tablet"),
        english!(DIRECTION, "Direction"),
        english!(DIRECTION_AUTO, "Auto direction"),
        english!(DIRECTION_LTR, "Left to right"),
        english!(DIRECTION_RTL, "Right to left"),
        english!(FILTER_ACTIONS, "Filter actions"),
        english!(FIND, "Find a preview or command"),
        english!(FULLSCREEN, "Fullscreen canvas"),
        english!(GRID, "Grid"),
        english!(GROUP_CANVAS, "Canvas"),
        english!(GROUP_NAVIGATE, "Navigate"),
        english!(GROUP_VIEW, "View"),
        english!(HIGH_CONTRAST, "High contrast"),
        english!(KEYBOARD_SHORTCUTS, "Keyboard shortcuts"),
        english!(LOCALE, "Locale"),
        english!(LOCALE_DEFAULT, "Default locale"),
        english!(MODE, "Mode"),
        english!(MODE_DEFAULT, "Default mode"),
        english!(MOVE_PANELS, "Move the panels"),
        english!(NEXT_COMPONENT, "Next component"),
        english!(NEXT_PREVIEW, "Next preview"),
        english!(NO_ACTIONS, "No actions yet"),
        english!(NO_ARGS, "No args"),
        english!(NO_MATCHES, "No matches"),
        english!(NO_MATCHING_ACTIONS, "No actions match the filter"),
        english!(NO_PREVIEWS, "No previews"),
        english!(
            NO_PREVIEWS_HINT,
            "Write a [preview] block in an .rsx file, or a preview! in Rust, and it is listed here."
        ),
        english!(NOTHING_SELECTED, "Nothing selected"),
        english!(OPEN_IN_CANVAS, "Open in canvas"),
        english!(OPEN_IN_EDITOR, "Open in editor"),
        english!(PANELS, "Panels"),
        english!(PANICKED, "This preview panicked"),
        english!(PRESET_DESKTOP, "Desktop"),
        english!(PRESET_LAPTOP, "Laptop"),
        english!(PRESET_MOBILE, "Mobile"),
        english!(PRESET_TABLET, "Tablet"),
        english!(PREVIEWS, "Previews"),
        english!(PREVIOUS_COMPONENT, "Previous component"),
        english!(PREVIOUS_PREVIEW, "Previous preview"),
        english!(PROJECT_VIEWPORTS, "Project"),
        english!(PROPS, "Props"),
        english!(REDUCED_MOTION, "Reduce motion"),
        english!(REMOUNT, "Remount"),
        english!(REQUIRED, "Required"),
        english!(RESET, "Reset"),
        english!(RESET_ALL, "Reset all"),
        english!(RESET_ARG, "Reset ", { arg }),
        english!(RESIZE_PANELS, "Resize the panels"),
        english!(RESIZE_SIDEBAR, "Resize the sidebar"),
        english!(ROTATE, "Rotate"),
        english!(RULERS, "Rulers"),
        english!(SEARCH, "Search previews"),
        english!(SHOW_CANVAS, "Show the canvas"),
        english!(SHOW_DOCS, "Show the docs"),
        english!(SOURCE_OF, "Source of ", { arg }),
        english!(TOGGLE_GRID, "Show or hide the grid"),
        english!(TOGGLE_PANELS, "Show or hide the panels"),
        english!(TOGGLE_RULERS, "Show or hide the rulers"),
        english!(TOGGLE_SIDEBAR, "Show or hide the sidebar"),
        english!(UNSET, "Unset"),
        english!(UNSET_ARG, "Unset ", { arg }),
        english!(VIEW, "View"),
        english!(VIEW_CANVAS, "Canvas"),
        english!(VIEW_DOCS, "Docs"),
        english!(VIEWPORT, "Viewport"),
        english!(VIEWPORT_CUSTOM, "Custom"),
        english!(VIEWPORT_FILL, "Fill"),
        english!(VIEWPORT_HEIGHT, "Viewport height"),
        english!(VIEWPORT_PRESETS, "Viewports"),
        english!(VIEWPORT_PREVIEW, "Preview size"),
        english!(VIEWPORT_RESET, "Reset the viewport"),
        english!(VIEWPORT_SIZE, "Viewport size"),
        english!(VIEWPORT_WIDTH, "Viewport width"),
        english!(WORKSHOP, "Workshop"),
        english!(ZOOM, "Zoom"),
        english!(ZOOM_FIT, "Fit"),
        english!(ZOOM_IN, "Zoom in"),
        english!(ZOOM_OUT, "Zoom out"),
        english!(ZOOM_PERCENT, { arg }, "%"),
        english!(ZOOM_RESET, "Zoom to fit"),
    ],
};

/// Resolves `key` for the active locale, preferring the application's `telar_workshop.<key>` message over the English one shipped here. Call it inside a reactive closure so a locale switch re-renders.
pub(crate) fn text(key: &str) -> String {
    translate_with_override(NAMESPACE, &CATALOG, key, &[])
}

/// [`text`] as a prop that re-reads on a locale switch.
pub(crate) fn reactive(key: &'static str) -> Reactive<String> {
    Reactive::of(move || text(key))
}

/// [`text`] for a message with one value in it, as `{arg}`: the name of a preview's arg, or a number.
pub(crate) fn text_with(key: &str, arg: &str) -> String {
    translate_with_override(NAMESPACE, &CATALOG, key, &[("arg", arg)])
}

#[cfg(test)]
#[path = "strings_test.rs"]
mod tests;
