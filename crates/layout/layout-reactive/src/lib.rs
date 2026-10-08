//! Layout made reactive: a taffy tree whose node rects are signals, so a widget re-renders when its box moves.

#![warn(rustdoc::broken_intra_doc_links)]

mod context;
mod direction;
mod surface_size;

pub use direction::{
    DirectionContext, DirectionGuard, current_direction, set_direction, set_surface_direction,
    use_direction, use_surface_direction,
};
pub use surface_size::{
    SurfaceSizeContext, SurfaceSizeGuard, set_safe_area_insets, set_surface_size, surface_size,
    use_safe_area_insets, use_surface_height, use_surface_size, use_surface_width,
};

pub use context::{
    Ancestors, LayoutContext, LayoutGuard, NodeRecording, ParentsContext, ParentsGuard,
    RelayoutHook, absolute_rect, after_layout, ancestors, attach_overlay, compute_layout,
    container_is_row, declared_css, detach_overlay, is_descendant_of, is_hidden,
    lay_out_against_surface, layout_passes, live_node_count, mark_dirty, measure_again,
    new_container, new_leaf, new_measured_leaf, on_relayout, overlay_viewport, parent,
    record_nodes, relayout_if_dirty, remeasure_since, remove_node, reset_layout_runtime,
    set_children, set_container_row, set_display, set_layout_style, set_leading_margin,
    set_min_height, set_overlay_host, set_sticky_view, track_display_none, track_layout,
};
pub use layout_core::{
    AlignItems, AvailableSpace, Css, Direction, JustifyContent, LayoutError, LayoutStyle,
    MeasureFn, MeasureInput, NodeId, SizeDimension, StickyAnchor, TemplateTrack,
};
