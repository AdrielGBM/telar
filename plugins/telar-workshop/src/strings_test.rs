use super::*;

#[test]
fn every_string_has_an_english_default() {
    for (key, english) in [
        (WORKSHOP, "Workshop"),
        (SEARCH, "Search previews"),
        (VIEW_CANVAS, "Canvas"),
        (VIEW_MATRIX, "Matrix"),
        (VIEW_DOCS, "Docs"),
        (PREVIEWS, "Previews"),
        (NO_MATCHES, "No matches"),
        (CLEAR_SEARCH, "Clear search"),
        (NO_PREVIEWS, "No previews"),
        (PANELS, "Panels"),
        (CONTROLS, "Controls"),
        (BUILD_FAILED, "This preview failed to build"),
        (
            NO_PREVIEWS_HINT,
            "Write a [preview] block in an .rsx file, or a preview! in Rust, and it is listed here.",
        ),
        (OPEN_IN_EDITOR, "Open in editor"),
        (PANICKED, "This preview panicked"),
        (REMOUNT, "Remount"),
        (CANVAS_TOOLS, "Canvas tools"),
        (VIEWPORT_HEIGHT, "Viewport height"),
        (VIEWPORT_RESET, "Reset the viewport"),
        (VIEWPORT_SIZE, "Viewport size"),
        (VIEWPORT_WIDTH, "Viewport width"),
        (COLUMN_DEFAULT, "Default"),
        (COLUMN_DESCRIPTION, "Description"),
        (COLUMN_NAME, "Name"),
        (COLUMN_VALUE, "Value"),
        (NO_ARGS, "No args"),
        (NOTHING_SELECTED, "Nothing selected"),
        (RESET, "Reset"),
        (RESET_ALL, "Reset all"),
        (UNSET, "Unset"),
        (RESIZE_PANELS, "Resize the panels"),
        (RESIZE_SIDEBAR, "Resize the sidebar"),
    ] {
        assert_eq!(text(key), english);
    }
}

#[test]
fn a_message_naming_an_arg_puts_the_arg_in() {
    assert_eq!(text_naming(RESET_ARG, "label"), "Reset label");
    assert_eq!(text_naming(UNSET_ARG, "note"), "Unset note");
}

#[test]
fn the_catalog_is_sorted_so_a_lookup_finds_every_key() {
    let keys: Vec<&str> = CATALOG.entries.iter().map(|entry| entry.key).collect();
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    assert_eq!(keys, sorted);
}
