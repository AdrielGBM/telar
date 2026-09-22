use geometry_core::Rect;
use layout_core::{LayoutStyle, SizeDimension};

use super::*;

// These hang rather than fail when the guard is gone, which is how the bug reached a desktop.
#[test]
fn an_overlay_is_refused_a_host_that_sits_inside_it() {
    reset_layout_runtime();
    let inner = new_container(LayoutStyle::new(), &[]).unwrap();
    let content = new_container(LayoutStyle::new(), &[inner]).unwrap();
    // What auto-detection can leave behind once a sub-root of the overlay has been laid out on its own.
    set_overlay_host(inner);

    assert!(
        !attach_overlay(content),
        "a host beneath the content it would carry closes a parent cycle"
    );
    assert_eq!(
        parent(content),
        None,
        "a refused attach leaves the content parentless rather than half-linked"
    );
}

#[test]
fn removing_a_node_leaves_no_link_pointing_at_it() {
    reset_layout_runtime();
    let child = new_container(LayoutStyle::new(), &[]).unwrap();
    let host = new_container(LayoutStyle::new(), &[child]).unwrap();
    assert_eq!(parent(child), Some(host));

    remove_node(host);

    assert_eq!(
        parent(child),
        None,
        "a link to a freed node outlives it and then names whatever reuses the id"
    );
}

#[test]
fn a_cycle_in_the_parent_links_ends_the_climb_instead_of_spinning() {
    reset_layout_runtime();
    let a = new_container(LayoutStyle::new(), &[]).unwrap();
    let b = new_container(LayoutStyle::new(), &[]).unwrap();
    let unrelated = new_container(LayoutStyle::new(), &[]).unwrap();
    with_parents(|p| {
        p.insert(a, b);
        p.insert(b, a);
    });

    let climbed: Vec<_> = ancestors(a).collect();
    assert!(
        climbed.iter().all(|node| *node == a || *node == b),
        "the climb stays inside the cycle it found: {climbed:?}"
    );
    assert!(
        !is_descendant_of(a, unrelated),
        "a question asked from inside a cycle answers instead of hanging"
    );
}

// Regression: taffy 0.11 measured a `max-width` box at its uncapped one-line intrinsic width, so this crate pinned every capped box to its resolved width and laid out a second time. That pass went with taffy 0.13; `telar/src/max_width_wrapping_test.rs` covers the same case through the real stack.
#[test]
fn a_maxwidth_box_measures_its_content_at_the_capped_width() {
    reset_layout_runtime();
    const TOTAL: f32 = 1200.0;
    const LINE: f32 = 20.0;
    let measure: layout_core::MeasureFn = Box::new(|available: f32| {
        let width = if available > 0.0 { available } else { TOTAL };
        (width, (TOTAL / width).ceil() * LINE)
    });
    let (text, text_rect) = new_measured_leaf(LayoutStyle::new(), measure).unwrap();
    let box_node =
        new_container(LayoutStyle::new().flex_column().max_width(400.0), &[text]).unwrap();
    let root = new_container(LayoutStyle::new().flex_column(), &[box_node]).unwrap();
    let box_rect = track_layout(box_node).unwrap();

    compute_layout(
        root,
        AvailableSpace::Definite(1000.0),
        AvailableSpace::Definite(1000.0),
    )
    .unwrap();

    assert_eq!(text_rect.get().width, 400.0);
    assert_eq!(
        box_rect.get().height,
        3.0 * LINE,
        "the box has to reserve the height its content takes at the width it was capped to"
    );
}

#[test]
fn maxwidth_box_reserves_height_for_wrapped_content() {
    reset_layout_runtime();
    let mut items = Vec::new();
    for _ in 0..4 {
        let (n, _) = new_leaf(
            LayoutStyle::new()
                .width(200.0)
                .height(100.0)
                .min_width(200.0)
                .flex_grow(1.0),
        )
        .unwrap();
        items.push(n);
    }
    let row = new_container(LayoutStyle::new().flex_row().flex_wrap().gap(24.0), &items).unwrap();
    let boxed = new_container(
        LayoutStyle::new()
            .flex_column()
            .width(SizeDimension::Percent(1.0))
            .max_width(500.0),
        &[row],
    )
    .unwrap();
    let page = new_container(
        LayoutStyle::new()
            .flex_column()
            .width(SizeDimension::Percent(1.0)),
        &[boxed],
    )
    .unwrap();
    compute_layout(
        page,
        AvailableSpace::Definite(900.0),
        AvailableSpace::MaxContent,
    )
    .unwrap();
    let box_rect = track_layout(boxed).unwrap().get();
    let row_rect = track_layout(row).unwrap().get();
    assert!(
        (box_rect.width - 500.0).abs() < 1.0,
        "box not capped: {box_rect:?}"
    );
    assert!(
        row_rect.height >= 200.0,
        "row did not wrap to 2 lines: {row_rect:?}"
    );
    assert!(
        box_rect.height >= row_rect.height - 0.5,
        "box too short for wrapped content: box={box_rect:?} row={row_rect:?}"
    );
}

#[test]
fn wrapped_flex_row_reserves_height_for_all_lines() {
    reset_layout_runtime();
    let mut cards = Vec::new();
    for _ in 0..4 {
        let (n, _) = new_leaf(
            LayoutStyle::new()
                .min_width(260.0)
                .height(100.0)
                .flex_grow(1.0),
        )
        .unwrap();
        cards.push(n);
    }
    let row = new_container(LayoutStyle::new().flex_row().flex_wrap().gap(24.0), &cards).unwrap();
    let (marker, _) = new_leaf(LayoutStyle::new().height(50.0)).unwrap();
    let col = new_container(LayoutStyle::new().flex_column().gap(20.0), &[row, marker]).unwrap();
    compute_layout(
        col,
        AvailableSpace::Definite(900.0),
        AvailableSpace::MaxContent,
    )
    .unwrap();
    let row_rect = track_layout(row).unwrap().get();
    let marker_rect = track_layout(marker).unwrap().get();
    assert!(
        row_rect.height >= 220.0,
        "wrapped row height {} should cover 2 lines (~224)",
        row_rect.height
    );
    assert!(
        marker_rect.y >= row_rect.y + row_rect.height - 0.5,
        "marker overlaps wrapped row: row.y={} row.h={} marker.y={}",
        row_rect.y,
        row_rect.height,
        marker_rect.y
    );
}

#[test]
fn wrapped_content_sized_cards_reserve_height() {
    reset_layout_runtime();
    let mut cards = Vec::new();
    for _ in 0..4 {
        let (inner, _) = new_leaf(LayoutStyle::new().height(100.0)).unwrap();
        let card = new_container(
            LayoutStyle::new()
                .flex_column()
                .min_width(260.0)
                .flex_grow(1.0),
            &[inner],
        )
        .unwrap();
        cards.push(card);
    }
    let row = new_container(LayoutStyle::new().flex_row().flex_wrap().gap(24.0), &cards).unwrap();
    let (marker, _) = new_leaf(LayoutStyle::new().height(50.0)).unwrap();
    let col = new_container(LayoutStyle::new().flex_column().gap(20.0), &[row, marker]).unwrap();
    compute_layout(
        col,
        AvailableSpace::Definite(900.0),
        AvailableSpace::MaxContent,
    )
    .unwrap();
    let row_rect = track_layout(row).unwrap().get();
    let marker_rect = track_layout(marker).unwrap().get();
    assert!(
        row_rect.height >= 220.0,
        "wrapped content-sized row height {} should cover 2 lines (~224)",
        row_rect.height
    );
    assert!(
        marker_rect.y >= row_rect.y + row_rect.height - 0.5,
        "marker overlaps row: row.y={} row.h={} marker.y={}",
        row_rect.y,
        row_rect.height,
        marker_rect.y
    );
}

#[test]
fn maxwidth_box_stable_across_recompute() {
    reset_layout_runtime();
    let mut items = Vec::new();
    for _ in 0..4 {
        let (n, _) = new_leaf(
            LayoutStyle::new()
                .width(200.0)
                .height(100.0)
                .min_width(200.0)
                .flex_grow(1.0),
        )
        .unwrap();
        items.push(n);
    }
    let row = new_container(LayoutStyle::new().flex_row().flex_wrap().gap(24.0), &items).unwrap();
    let boxed = new_container(
        LayoutStyle::new()
            .flex_column()
            .width(SizeDimension::Percent(1.0))
            .max_width(500.0),
        &[row],
    )
    .unwrap();
    let page = new_container(
        LayoutStyle::new()
            .flex_column()
            .width(SizeDimension::Percent(1.0)),
        &[boxed],
    )
    .unwrap();

    let space = (AvailableSpace::Definite(900.0), AvailableSpace::MaxContent);
    compute_layout(page, space.0, space.1).unwrap();
    let first = track_layout(boxed).unwrap().get();

    mark_dirty(page).unwrap();
    compute_layout(page, space.0, space.1).unwrap();
    let second = track_layout(boxed).unwrap().get();

    assert!(
        (second.width - 500.0).abs() < 1.0,
        "box not capped on recompute: {second:?}"
    );
    assert!(
        (first.width - second.width).abs() < 0.5 && (first.height - second.height).abs() < 0.5,
        "box layout drifted across recompute: first={first:?} second={second:?}"
    );
}

#[test]
fn hidden_maxwidth_box_stays_hidden_after_a_resize() {
    reset_layout_runtime();
    let mut items = Vec::new();
    for _ in 0..4 {
        let (n, _) = new_leaf(
            LayoutStyle::new()
                .width(200.0)
                .height(100.0)
                .min_width(200.0)
                .flex_grow(1.0),
        )
        .unwrap();
        items.push(n);
    }
    let row = new_container(LayoutStyle::new().flex_row().flex_wrap().gap(24.0), &items).unwrap();
    let boxed = new_container(
        LayoutStyle::new()
            .flex_column()
            .width(SizeDimension::Percent(1.0))
            .max_width(500.0),
        &[row],
    )
    .unwrap();
    let page = new_container(
        LayoutStyle::new()
            .flex_column()
            .width(SizeDimension::Percent(1.0)),
        &[boxed],
    )
    .unwrap();

    compute_layout(
        page,
        AvailableSpace::Definite(900.0),
        AvailableSpace::MaxContent,
    )
    .unwrap();

    set_display(boxed, false);
    mark_dirty(page).unwrap();
    assert!(is_hidden(boxed), "hidden right after set_display");

    compute_layout(
        page,
        AvailableSpace::Definite(700.0),
        AvailableSpace::MaxContent,
    )
    .unwrap();
    assert!(
        is_hidden(boxed),
        "resize must not revert the out-of-band hide"
    );
}

/// **A style that says «shown» is heard, however the node was hidden before.** The declared answer and the out-of-band one used to be one flag OR-ed into whatever came next, so a node taken out of flow once could never be put back by a style — which is what a `shown:` re-resolved from what it reads does on every change. The out-of-band answer still survives a restyle, because a fresh style cannot know it was ever given.
#[test]
fn a_style_may_show_what_a_style_hid_and_may_not_show_what_set_display_hid() {
    reset_layout_runtime();
    let node = new_container(
        LayoutStyle::new().width(100.0).height(10.0).shown(false),
        &[],
    )
    .unwrap();
    let page = new_container(LayoutStyle::new().flex_column().width(200.0), &[node]).unwrap();
    compute_layout(
        page,
        AvailableSpace::Definite(200.0),
        AvailableSpace::MaxContent,
    )
    .unwrap();
    assert!(is_hidden(node), "the style declared it out of flow");

    set_layout_style(
        node,
        LayoutStyle::new().width(100.0).height(10.0).shown(true),
    )
    .unwrap();
    compute_layout(
        page,
        AvailableSpace::Definite(200.0),
        AvailableSpace::MaxContent,
    )
    .unwrap();
    assert!(
        !is_hidden(node),
        "and the next style put it back, which is what a re-resolved `shown:` asks for"
    );

    set_display(node, false);
    set_layout_style(
        node,
        LayoutStyle::new().width(100.0).height(10.0).shown(true),
    )
    .unwrap();
    compute_layout(
        page,
        AvailableSpace::Definite(200.0),
        AvailableSpace::MaxContent,
    )
    .unwrap();
    assert!(
        is_hidden(node),
        "an out-of-band hide is nobody else's to lift: the style never knew about it"
    );
}

#[test]
fn auto_root_fills_definite_width() {
    reset_layout_runtime();
    let (child, _) = new_leaf(LayoutStyle::new().height(40.0)).unwrap();
    let page = new_container(LayoutStyle::new().flex_column(), &[child]).unwrap();
    compute_layout(
        page,
        AvailableSpace::Definite(1000.0),
        AvailableSpace::MaxContent,
    )
    .unwrap();
    let w = track_layout(page).unwrap().get().width;
    assert!(
        (w - 1000.0).abs() < 1.0,
        "auto root did not fill width: {w}"
    );
}

#[test]
fn hidden_child_collapses_to_zero_rect() {
    reset_layout_runtime();
    let (a, _) = new_leaf(LayoutStyle::new().width(50.0).height(30.0)).unwrap();
    let (b, b_rect) = new_leaf(LayoutStyle::new().width(50.0).height(30.0)).unwrap();
    let root = new_container(LayoutStyle::new().flex_column(), &[a, b]).unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();
    assert!(b_rect.get().height > 0.0, "b should start visible");

    set_display(b, false);
    mark_dirty(root).unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();
    let r = b_rect.get();
    assert_eq!(
        (r.width, r.height),
        (0.0, 0.0),
        "hidden child not collapsed: {r:?}"
    );
}

#[test]
fn hidden_subtree_collapses_descendants() {
    reset_layout_runtime();
    let (grandchild, gc_rect) = new_leaf(LayoutStyle::new().width(40.0).height(20.0)).unwrap();
    let section = new_container(LayoutStyle::new().flex_column(), &[grandchild]).unwrap();
    let root = new_container(LayoutStyle::new().flex_column(), &[section]).unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();
    assert!(gc_rect.get().width > 0.0, "grandchild should start visible");

    set_display(section, false);
    mark_dirty(root).unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();
    let r = gc_rect.get();
    assert_eq!(
        (r.width, r.height),
        (0.0, 0.0),
        "descendant of hidden section not collapsed: {r:?}"
    );
}

#[test]
fn auto_root_with_max_width_fills_capped() {
    reset_layout_runtime();
    let (child, _) = new_leaf(LayoutStyle::new().height(40.0)).unwrap();
    let page = new_container(LayoutStyle::new().flex_column().max_width(600.0), &[child]).unwrap();
    compute_layout(
        page,
        AvailableSpace::Definite(1000.0),
        AvailableSpace::MaxContent,
    )
    .unwrap();
    let w = track_layout(page).unwrap().get().width;
    assert!((w - 600.0).abs() < 1.0, "capped fill failed: {w}");
    compute_layout(
        page,
        AvailableSpace::Definite(400.0),
        AvailableSpace::MaxContent,
    )
    .unwrap();
    let w = track_layout(page).unwrap().get().width;
    assert!((w - 400.0).abs() < 1.0, "sub-cap fill failed: {w}");
}

#[test]
fn centered_capped_column_tracks_width() {
    reset_layout_runtime();
    let (child, _) = new_leaf(LayoutStyle::new().height(40.0)).unwrap();
    let inner = new_container(
        LayoutStyle::new()
            .flex_column()
            .width(SizeDimension::Percent(1.0))
            .max_width(960.0),
        &[child],
    )
    .unwrap();
    let outer = new_container(
        LayoutStyle::new()
            .flex_column()
            .align_items(layout_core::AlignItems::CENTER),
        &[inner],
    )
    .unwrap();
    let inner_rect = track_layout(inner).unwrap();
    let outer_rect = track_layout(outer).unwrap();
    compute_layout(
        outer,
        AvailableSpace::Definite(1400.0),
        AvailableSpace::MaxContent,
    )
    .unwrap();
    assert!(
        (outer_rect.get().width - 1400.0).abs() < 1.0,
        "outer fill: {}",
        outer_rect.get().width
    );
    assert!(
        (inner_rect.get().width - 960.0).abs() < 1.0,
        "inner cap: {}",
        inner_rect.get().width
    );
    assert!(
        (inner_rect.get().x - 220.0).abs() < 1.0,
        "inner centered: {}",
        inner_rect.get().x
    );
    compute_layout(
        outer,
        AvailableSpace::Definite(700.0),
        AvailableSpace::MaxContent,
    )
    .unwrap();
    assert!(
        (inner_rect.get().width - 700.0).abs() < 1.0,
        "inner tracks narrow: {}",
        inner_rect.get().width
    );
    assert!(
        inner_rect.get().x.abs() < 1.0,
        "no margin when full: {}",
        inner_rect.get().x
    );
}

// set_min_height grows a content-measured leaf to fill a viewport it would otherwise underflow (the notebook editor's fill-the-viewport trick); a leaf whose content already exceeds the floor is untouched.
#[test]
fn set_min_height_grows_short_measured_leaf() {
    reset_layout_runtime();
    let (leaf, rect) = new_measured_leaf(
        LayoutStyle::new().width(SizeDimension::Percent(1.0)),
        Box::new(|_w| (0.0, 20.0)),
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new()
            .flex_column()
            .width(SizeDimension::Percent(1.0)),
        &[leaf],
    )
    .unwrap();
    let space = (AvailableSpace::Definite(300.0), AvailableSpace::MaxContent);
    compute_layout(root, space.0, space.1).unwrap();
    assert!(
        (rect.get().height - 20.0).abs() < 0.5,
        "starts at its content height: {:?}",
        rect.get()
    );

    set_min_height(leaf, 200.0);
    compute_layout(root, space.0, space.1).unwrap();
    assert!(
        (rect.get().height - 200.0).abs() < 0.5,
        "min_height fills the short leaf: {:?}",
        rect.get()
    );

    set_min_height(leaf, 0.0);
    compute_layout(root, space.0, space.1).unwrap();
    assert!(
        (rect.get().height - 20.0).abs() < 0.5,
        "a zero floor restores the content height: {:?}",
        rect.get()
    );
}

// Regression: set_layout_style (what a styled_by closure calls on every reactive re-run) used to overwrite the node's whole style, discarding an unrelated out-of-band set_min_height. No max_width involved.
#[test]
fn min_height_survives_a_later_set_layout_style() {
    reset_layout_runtime();
    let (leaf, rect) = new_measured_leaf(
        LayoutStyle::new().width(SizeDimension::Percent(1.0)),
        Box::new(|_w| (0.0, 20.0)),
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new()
            .flex_column()
            .width(SizeDimension::Percent(1.0)),
        &[leaf],
    )
    .unwrap();
    let space = (AvailableSpace::Definite(300.0), AvailableSpace::MaxContent);
    compute_layout(root, space.0, space.1).unwrap();

    set_min_height(leaf, 200.0);
    compute_layout(root, space.0, space.1).unwrap();
    assert!(
        (rect.get().height - 200.0).abs() < 0.5,
        "min_height applied: {:?}",
        rect.get()
    );

    set_layout_style(leaf, LayoutStyle::new().width(SizeDimension::Percent(1.0))).unwrap();
    compute_layout(root, space.0, space.1).unwrap();
    assert!(
        (rect.get().height - 200.0).abs() < 0.5,
        "min_height survives a later set_layout_style: {:?}",
        rect.get()
    );
}

#[test]
fn ctx_register_leaf_returns_ok() {
    reset_layout_runtime();
    let result = new_leaf(LayoutStyle::new());
    assert!(result.is_ok(), "registering a leaf in a fresh runtime");
}

#[test]
fn ctx_new_container_returns_ok() {
    reset_layout_runtime();
    let leaf_result = new_leaf(LayoutStyle::new());
    assert!(leaf_result.is_ok(), "the child leaf builds");
    let (leaf, _) = leaf_result.unwrap();
    let container_result = new_container(LayoutStyle::new(), &[leaf]);
    assert!(container_result.is_ok(), "and the container that holds it");
}

#[test]
fn ctx_register_leaf_returns_zero_rect() {
    reset_layout_runtime();
    let (_node, rect) = new_leaf(LayoutStyle::new()).unwrap();
    assert_eq!(rect.get(), Rect::default());
}

#[test]
fn ctx_compute_updates_rect() {
    reset_layout_runtime();
    let (leaf, rect) = new_leaf(LayoutStyle::new().width(100.0).height(50.0)).unwrap();
    let root = new_container(
        LayoutStyle::new().flex_row().width(200.0).height(100.0),
        &[leaf],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();
    assert_eq!(rect.get().width, 100.0);
    assert_eq!(rect.get().height, 50.0);
}

#[test]
fn setting_the_direction_signal_reaches_the_engine_on_the_next_layout_pass() {
    reset_layout_runtime();
    crate::set_direction(layout_core::Direction::Ltr);
    let (first, first_rect) = new_leaf(LayoutStyle::new().width(40.0).height(10.0)).unwrap();
    let (second, second_rect) = new_leaf(LayoutStyle::new().width(40.0).height(10.0)).unwrap();
    let root = new_container(
        LayoutStyle::new().flex_row().width(200.0).height(100.0),
        &[first, second],
    )
    .unwrap();
    let space = || {
        (
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
    };
    let (w, h) = space();
    compute_layout(root, w, h).unwrap();
    assert_eq!(first_rect.get().x, 0.0);
    assert_eq!(second_rect.get().x, 40.0);

    crate::set_direction(layout_core::Direction::Rtl);
    mark_dirty(root).unwrap();
    let (w, h) = space();
    compute_layout(root, w, h).unwrap();
    assert_eq!(first_rect.get().x, 160.0, "the row now starts at the right");
    assert_eq!(second_rect.get().x, 120.0);
    crate::set_direction(layout_core::Direction::Ltr);
}

// An overlay's content, attached to the host, fills the viewport — not the small box it was declared in.
#[test]
fn attached_overlay_fills_host_viewport_not_its_small_parent() {
    reset_layout_runtime();
    let (small, _) = new_leaf(LayoutStyle::new().width(50.0).height(50.0)).unwrap();
    let root = new_container(LayoutStyle::new().flex_column(), &[small]).unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(800.0),
        AvailableSpace::Definite(600.0),
    )
    .unwrap();

    let (inner, inner_rect) = new_leaf(
        LayoutStyle::new()
            .width(SizeDimension::Percent(1.0))
            .height(SizeDimension::Percent(1.0)),
    )
    .unwrap();
    let content = new_container(LayoutStyle::new().absolute_fill(), &[inner]).unwrap();
    assert!(
        attach_overlay(content),
        "the host must be set after the first compute"
    );
    relayout_if_dirty();

    let r = inner_rect.get();
    assert!(
        (r.width - 800.0).abs() < 0.5 && (r.height - 600.0).abs() < 0.5,
        "portal fills the viewport, not its 50px parent: {r:?}"
    );

    detach_overlay(content);
    remove_node(content);
    relayout_if_dirty();
}

// Reproduces the sandbox shell's coordinate trap: a `[sidebar | content]` window root, then the `content` computed AGAIN as its own root (for scroll-height measurement) — which rewrites the content subtree's rect signals to content-local coords. `absolute_rect` must still report a trigger's WINDOW-absolute position (past the sidebar), so a portaled dropdown anchors correctly instead of landing over the sidebar.
#[test]
fn absolute_rect_stays_window_absolute_across_a_separate_content_root() {
    reset_layout_runtime();
    let (sidebar, _) = new_leaf(LayoutStyle::new().width(248.0).height(600.0)).unwrap();
    let (trigger, trigger_sig) = new_leaf(LayoutStyle::new().width(120.0).height(30.0)).unwrap();
    let content =
        new_container(LayoutStyle::new().flex_column().flex_grow(1.0), &[trigger]).unwrap();
    let root = new_container(LayoutStyle::new().flex_row(), &[sidebar, content]).unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(1000.0),
        AvailableSpace::Definite(600.0),
    )
    .unwrap();
    set_overlay_host(root);
    assert!(
        (absolute_rect(trigger).unwrap().x - 248.0).abs() < 1.0,
        "abs x should be past the 248px sidebar: {:?}",
        absolute_rect(trigger)
    );

    mark_dirty(content).unwrap();
    compute_layout(
        content,
        AvailableSpace::Definite(752.0),
        AvailableSpace::MaxContent,
    )
    .unwrap();
    assert!(
        trigger_sig.get().x < 1.0,
        "the rect signal is now content-local (~0): {:?}",
        trigger_sig.get()
    );
    assert!(
        (absolute_rect(trigger).unwrap().x - 248.0).abs() < 1.0,
        "absolute_rect must stay window-absolute across the sub-root compute: {:?}",
        absolute_rect(trigger)
    );
}

#[test]
fn removing_a_node_frees_everything_beneath_it() {
    reset_layout_runtime();
    let baseline = live_node_count();
    let (a, _) = new_leaf(LayoutStyle::new()).unwrap();
    let (b, _) = new_leaf(LayoutStyle::new()).unwrap();
    let inner = new_container(LayoutStyle::new(), &[a, b]).unwrap();
    let root = new_container(LayoutStyle::new(), &[inner]).unwrap();
    assert_eq!(live_node_count(), baseline + 4);

    remove_node(root);

    assert_eq!(live_node_count(), baseline);
    assert!(track_layout(a).is_none(), "its rect signal went with it");
    assert_eq!(parent(a), None, "and so did its parent link");
    remove_node(a);
}

#[test]
fn a_dropped_recording_frees_what_it_recorded_and_a_kept_one_does_not() {
    reset_layout_runtime();
    let baseline = live_node_count();

    let kept = record_nodes();
    let (survivor, _) = new_leaf(LayoutStyle::new()).unwrap();
    kept.keep();

    let recording = record_nodes();
    let (loose, _) = new_leaf(LayoutStyle::new()).unwrap();
    let _attached = new_container(LayoutStyle::new(), &[loose]).unwrap();
    drop(recording);

    assert_eq!(live_node_count(), baseline + 1);
    assert!(track_layout(survivor).is_some());
}

/// A build that writes a signal flushes effects elsewhere, and what those create is theirs, not the build's to free.
#[test]
fn a_recording_frees_only_what_its_owner_built() {
    reset_layout_runtime();
    let elsewhere = reactive_core::owner_scope();
    let elsewhere_id = elsewhere.id();
    drop(elsewhere);
    let baseline = live_node_count();

    let building = reactive_core::owner_scope();
    let recording = record_nodes();
    let (mine, _) = new_leaf(LayoutStyle::new()).unwrap();
    let (theirs, _) =
        reactive_core::with_owner(Some(elsewhere_id), || new_leaf(LayoutStyle::new())).unwrap();
    drop(recording);
    drop(building);

    assert!(track_layout(mine).is_none());
    assert!(track_layout(theirs).is_some());
    assert_eq!(live_node_count(), baseline + 1);
}

#[test]
fn an_unwind_through_a_recording_frees_what_it_recorded() {
    reset_layout_runtime();
    let baseline = live_node_count();
    let outcome = std::panic::catch_unwind(|| {
        let _recording = record_nodes();
        let _ = new_leaf(LayoutStyle::new()).unwrap();
        panic!("mid-build");
    });
    assert!(outcome.is_err());
    assert_eq!(live_node_count(), baseline);
}

/// A list elsewhere in the tree reconciles in a flush a build set off and keeps its new rows: they are that list's, and the build failing later must not free them.
#[test]
fn a_kept_inner_recording_hands_its_nodes_only_to_an_enclosing_recording_that_owns_them() {
    reset_layout_runtime();
    let elsewhere = reactive_core::owner_scope();
    let baseline = live_node_count();

    let building = reactive_core::owner_scope();
    let outer = record_nodes();
    let (theirs, _) = reactive_core::with_owner(Some(elsewhere.id()), || {
        let inner = record_nodes();
        let created = new_leaf(LayoutStyle::new()).unwrap();
        inner.keep();
        created
    });
    let inner = record_nodes();
    let (mine, _) = new_leaf(LayoutStyle::new()).unwrap();
    inner.keep();
    drop(outer);
    drop(building);

    assert!(
        track_layout(mine).is_none(),
        "a kept row of the failed build goes with it"
    );
    assert!(
        track_layout(theirs).is_some(),
        "a row another list kept stays"
    );
    assert_eq!(live_node_count(), baseline + 1);
    drop(elsewhere);
}

/// Node ids are per surface, so a recording opened on one surface must not free an id minted on another — it names a different node there.
#[test]
fn a_recording_never_frees_a_node_built_on_another_surface() {
    use reactive_core::{SurfaceEnterGuard, SurfaceHandle, set_current_surface};

    type Worlds = (LayoutContext, ParentsContext);
    fn enter(handle: SurfaceHandle, worlds: &'static Worlds) -> SurfaceEnterGuard {
        let prev = set_current_surface(handle);
        let layout = worlds.0.enter();
        let parents = worlds.1.enter();
        SurfaceEnterGuard::new(move || {
            drop(parents);
            drop(layout);
            set_current_surface(prev);
        })
    }
    let (a, b) = (SurfaceHandle(1), SurfaceHandle(2));
    let a_worlds: &'static Worlds =
        Box::leak(Box::new((LayoutContext::new(), ParentsContext::new())));
    let b_worlds: &'static Worlds =
        Box::leak(Box::new((LayoutContext::new(), ParentsContext::new())));
    reactive_core::set_surface_enter_hook(move |handle| match handle {
        h if h == a => enter(a, a_worlds),
        h if h == b => enter(b, b_worlds),
        _ => SurfaceEnterGuard::noop(),
    });

    let on_a = a.enter();
    let (a_survivor, _) = new_leaf(LayoutStyle::new()).unwrap();
    let recording = record_nodes();
    let (b_loose, b_kept) = {
        let _on_b = b.enter();
        let (loose, _) = new_leaf(LayoutStyle::new()).unwrap();
        let inner = record_nodes();
        let (kept, _) = new_leaf(LayoutStyle::new()).unwrap();
        inner.keep();
        (loose, kept)
    };
    let (a_failed, _) = new_leaf(LayoutStyle::new()).unwrap();
    drop(recording);

    assert!(track_layout(a_survivor).is_some());
    assert!(track_layout(a_failed).is_none());
    assert_eq!(live_node_count(), 1);
    drop(on_a);
    let _on_b = b.enter();
    assert!(track_layout(b_loose).is_some());
    assert!(track_layout(b_kept).is_some());
    assert_eq!(live_node_count(), 2);
}

#[test]
fn a_sub_root_under_a_fixed_size_parent_is_laid_out_as_asked() {
    reset_layout_runtime();
    let (leaf, leaf_rect) = new_leaf(LayoutStyle::new().width(40.0).height(10.0)).unwrap();
    let content = new_container(LayoutStyle::new().flex_column(), &[leaf]).unwrap();
    let frame = new_container(
        LayoutStyle::new()
            .width(300.0)
            .height(200.0)
            .padding_all(10.0),
        &[content],
    )
    .unwrap();
    let frame_rect = track_layout(frame).unwrap();
    let content_rect = track_layout(content).unwrap();

    compute_layout(
        content,
        AvailableSpace::Definite(80.0),
        AvailableSpace::Definite(30.0),
    )
    .unwrap();

    assert_eq!(content_rect.get(), Rect::new(0.0, 0.0, 80.0, 30.0));
    assert_eq!(leaf_rect.get(), Rect::new(0.0, 0.0, 40.0, 10.0));
    assert_eq!(
        frame_rect.get(),
        Rect::default(),
        "the fixed-size parent was laid out in place of the root that was asked for"
    );
}

#[test]
fn a_resize_relays_out_a_surface_fraction_without_anyone_marking_it() {
    reset_layout_runtime();
    crate::set_surface_size(geometry_core::Size::new(1000.0, 800.0));
    let (half, half_rect) = new_leaf(
        LayoutStyle::new()
            .width(SizeDimension::SurfaceWidth(0.5))
            .height(SizeDimension::SurfaceHeight(0.1)),
    )
    .unwrap();
    let root = new_container(LayoutStyle::new().flex_column(), &[half]).unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(1000.0),
        AvailableSpace::Definite(800.0),
    )
    .unwrap();
    assert_eq!(half_rect.get().width, 500.0);
    assert_eq!(half_rect.get().height, 80.0);

    crate::set_surface_size(geometry_core::Size::new(600.0, 400.0));
    relayout_if_dirty();
    assert_eq!(
        half_rect.get().width,
        300.0,
        "the root's space is unchanged, so only the surface can have moved it"
    );
    assert_eq!(half_rect.get().height, 40.0);
}

#[test]
fn a_document_hears_a_surface_fraction_as_the_pixels_it_resolved_to() {
    reset_layout_runtime();
    crate::set_surface_size(geometry_core::Size::new(1000.0, 800.0));
    let (node, _) = new_leaf(LayoutStyle::new().width(SizeDimension::SurfaceWidth(0.25))).unwrap();
    let css = declared_css(node).unwrap();
    assert!(css.as_str().contains("width:250px;"), "{css}");
}
