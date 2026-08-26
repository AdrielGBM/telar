pub use layout_reactive::overlay_viewport;

/// Empties the layout runtime for a fresh tree — and the cascade with it.
///
/// A replaced `LayoutRuntime` is a fresh `SlotMap` whose versions start over, so the next tree is handed the
/// previous tree's `NodeId`s exactly. A declaration left behind therefore lands on whatever the next tree
/// builds on that id, which never shows up as a failure: the text under a node nobody declared for simply
/// comes out the wrong size.
///
/// **What this cannot do for you: dispose the previous tree's owners.** Anything still running from the old
/// tree — an effect styling a node, a keyboard walk reading one — names ids the new tree now owns. The
/// surface root is not the thing to dispose here, because it holds app-lifetime state that has nothing to do
/// with the tree being replaced. Whoever mounted the old tree scoped it, and whoever replaces it disposes
/// that scope.
pub fn reset_layout_runtime() {
    crate::inherit::reset_cascade();
    layout_reactive::reset_layout_runtime();
}
pub use layout_reactive::{
    NodeId, absolute_rect, attach_overlay, compute_layout, container_is_row, current_direction,
    detach_overlay, is_descendant_of, mark_dirty, new_container, new_leaf, new_measured_leaf,
    relayout_if_dirty, remove_node, set_children, set_container_row, set_direction, set_display,
    set_layout_style, set_leading_margin, set_min_height, set_overlay_host, track_layout,
    use_direction,
};
