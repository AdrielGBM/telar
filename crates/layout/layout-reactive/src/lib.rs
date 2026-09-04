//! Layout made reactive: a taffy tree whose node rects are signals, so a widget re-renders when its box moves.

mod context;
mod direction;

pub use direction::{current_direction, set_direction, use_direction};

pub use context::{
    Ancestors, LayoutContext, LayoutGuard, ParentsContext, ParentsGuard, absolute_rect, ancestors,
    attach_overlay, compute_layout, container_is_row, declared_css, detach_overlay,
    is_descendant_of, is_hidden, mark_dirty, new_container, new_leaf, new_measured_leaf,
    overlay_viewport, parent, relayout_if_dirty, remove_node, reset_layout_runtime, set_children,
    set_container_row, set_display, set_layout_style, set_leading_margin, set_min_height,
    set_overlay_host, track_layout,
};
pub use layout_core::{
    AlignItems, AvailableSpace, Css, Direction, JustifyContent, LayoutError, LayoutStyle,
    MeasureFn, NodeId, SizeDimension, TemplateTrack,
};
