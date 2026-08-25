pub use layout_reactive::overlay_viewport;

/// Empties the layout runtime for a fresh tree — and the cascade with it.
///
/// The two have to go together, because the cascade is keyed by `NodeId` and the runtime recycles those. A
/// declaration left behind lands on whatever the next tree happens to build on that id, and the widget that
/// made it withdraws one it no longer owns when it is finally dropped. Neither shows up as a failure: the
/// text under a node nobody declared for simply comes out the wrong size.
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
