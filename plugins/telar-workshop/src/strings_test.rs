use super::*;

#[test]
fn every_string_has_an_english_default() {
    for (key, english) in [
        (WORKSHOP, "Workshop"),
        (SEARCH, "Search previews"),
        (VIEW_CANVAS, "Canvas"),
        (VIEW_DOCS, "Docs"),
        (VIEW_MATRIX, "Matrix"),
        (SHOW_MATRIX, "Show the matrix"),
        (TOGGLE_MEASURE, "Show or hide the measure"),
        (TOGGLE_OUTLINE, "Show or hide the outline"),
        (MATRIX_OWN, "The preview's matrix"),
        (MATRIX_CUSTOM, "Custom axes"),
        (PLAY_PASSED, "Play passed"),
        (SNAPSHOT_DRIFTED, "Snapshot drifted"),
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
        (ACTIONS, "Actions"),
        (CLEAR_ACTIONS, "Clear"),
        (COLUMN_TYPE, "Type"),
        (FILTER_ACTIONS, "Filter actions"),
        (NO_ACTIONS, "No actions yet"),
        (NO_MATCHING_ACTIONS, "No actions match the filter"),
        (OPEN_IN_CANVAS, "Open in canvas"),
        (PROPS, "Props"),
        (REQUIRED, "Required"),
    ] {
        assert_eq!(text(key), english);
    }
}

#[test]
fn a_message_naming_an_arg_puts_the_arg_in() {
    assert_eq!(text_with(RESET_ARG, "label"), "Reset label");
    assert_eq!(text_with(UNSET_ARG, "note"), "Unset note");
    assert_eq!(text_with(SOURCE_OF, "Primary"), "Source of Primary");
    assert_eq!(text_with(A11Y_ERRORS, "2"), "A11y errors: 2");
    assert_eq!(
        text_with(MATRIX_NAMED, "themes"),
        "The preview's themes matrix"
    );
}

#[test]
fn a_message_with_several_values_puts_each_in_its_place() {
    assert_eq!(
        text_with_values(MATRIX_CAPPED, &[("shown", "64"), ("total", "75")]),
        "Showing the first 64 of 75 cells: narrow the matrix to see the rest"
    );
}

#[test]
fn the_catalog_is_sorted_so_a_lookup_finds_every_key() {
    CATALOG.assert_sorted();
}
