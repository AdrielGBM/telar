//! The widget catalogue's foundation: the primitives every component is built from, and the ambient state they share — focus, pointer, overlays, the cascade and the per-surface worlds all of it lives in.

#![warn(rustdoc::broken_intra_doc_links)]

pub mod accessibility;
mod anchor_line;
mod annotation;
mod arrival_margin;
#[cfg(feature = "async-assets")]
mod async_asset;
mod border;
mod breakpoints;
mod canvas;
mod caret;
mod child_host;
mod composition;
mod container;
mod context;
mod cursor;
pub mod dismiss;
mod disposal;
mod drag;
mod element;
mod error_boundary;
mod fixed_layer;
mod fling;
pub mod focus;
mod foreign_surface;
mod frame_focus;
mod guest_surface;
mod height_index;
mod image;
mod inherit;
mod input;
mod input_region;
mod kept;
mod keyboard;
mod keynav;
mod layout_item;
mod layout_leaf;
mod layout_transition;
mod lazy;
mod link;
mod mask;
mod movable;
mod named_overlay;
pub mod overlay;
mod page_anchor;
mod path;
mod pointer;
mod presence;
mod press;
mod reactive_list;
mod rect;
mod reorder;
mod row_keys;
mod scroll_area;
mod scroll_page;
mod scroll_timeline;
mod scroll_viewports;
mod serial;
mod slots;
mod step;
mod styled_container;
mod surface;
mod surface_canvas;
mod surface_context;
mod surface_font;
mod surface_frame;
mod surface_style;
mod surface_title;
#[cfg(feature = "svg")]
mod svg;
#[cfg(test)]
mod test_support;
mod text;
mod text_area;
mod text_edit;
mod text_editor;
mod text_fit;
mod text_input;
mod text_metrics;
mod text_place;
mod theme_provider;
mod transform_origin;
pub use text_metrics::{
    SINGLE_LINE_LEADING, line_box, single_line_box, use_text_metrics_generation,
};
mod virtual_list;
mod visual_lines;
mod window_root;

pub use anchor_line::use_anchor_at;
pub use annotation::Accessible;
#[cfg(feature = "async-assets")]
pub use async_asset::{
    AssetCache, AssetDecoder, AssetError, AssetKey, AssetLoader, AssetState, AssetTransport, Reply,
};
pub use border::{logical_border_radius, logical_border_widths};
pub use breakpoints::{Breakpoints, breakpoint};
pub use canvas::Canvas;
pub use child_host::{ChildSlot, fragment, fragment_positional};
pub use container::Container;
pub use context::{
    NodeId, RelayoutHook, absolute_rect, compute_layout, current_direction, live_node_count,
    mark_dirty, new_container, new_leaf, on_relayout, overlay_viewport, relayout_if_dirty,
    remove_node, reset_layout_runtime, set_children, set_direction, set_display, set_min_height,
    set_overlay_host, set_safe_area_insets, set_surface_maximized, set_surface_size, surface_size,
    track_layout, use_direction, use_safe_area_insets, use_surface_height, use_surface_maximized,
    use_surface_size, use_surface_width,
};
pub use cursor::requested_cursor;
pub use dismiss::{
    DismissRegistration, confirm_top, dismiss_depth, dismiss_top, register_transaction,
    register_transaction_within, use_dismiss_depth,
};
pub use disposal::OwnedBuild;
pub use drag::{DragAxis, DragStart, drag_start, drag_travel};
pub use error_boundary::{BuildFailure, ErrorBoundary};
pub use fixed_layer::FixedLayer;
pub use foreign_surface::{ForeignBoundary, ForeignSurface};
pub use frame_focus::{FrameFocus, KeyRoute};
pub use guest_surface::{GuestAudit, GuestSurface};
pub use height_index::HeightIndex;
pub use image::Image;
pub use inherit::{
    AuthoredText, Inherited, author, context, declare, inherited_text_style, undeclare,
};
pub use input::Input;
pub use input_region::{interactive_rects, is_hidden, visible_rect};
pub use kept::kept;
pub use keyboard::{
    chord_pressed, end_frame as end_keyboard_frame, key_held, key_pressed, modifiers,
    observe as observe_keyboard, reset as reset_keyboard, shortcut_key,
};
pub use keynav::{KeyNav, KeyNavMove, key_nav_apply, key_nav_apply_grid};
pub use layout_item::{Clip, ClipAxis, ClipPointer, ClippedItem, IntoClip, LayoutItem, box_item};
pub use layout_leaf::LayoutLeaf;
pub use layout_transition::{LayoutTransition, animate_layout};
pub use lazy::Lazy;
pub use link::{
    AnchorRegistration, activate_box, activate_run, follow, follow_beside, follow_pressed,
    has_anchor, reader_moved, register_anchor, reveal_anchor,
};
pub use mask::Mask;
pub use movable::{Movable, MovableOutlet};
pub use named_overlay::{close as close_overlay, open as open_overlay, state as overlay_state};
pub use overlay::{AnchorAlign, Anchoring, Overlay, Placement, anchor_rect};
pub use page_anchor::PageAnchor;
pub use path::Path;
pub use pointer::{
    PointerButtons, covered, observe_pointer, pointer_buttons, pointer_position, reset_pointer,
    transform_pointer,
};
pub use presence::{Presence, Transition, exits_in_flight};
pub use reactive_list::ReactiveList;
pub use rect::Rectangle;
pub use reorder::{Axis, apply_move, insertion_index};
pub use scroll_area::{LayoutScrollArea, ScrollViewport, ScrollbarStyle};
pub use scroll_page::ScrollPage;
pub use scroll_timeline::{
    ScrollLinked, ViewRange, range_progress, scroll_progress, scroll_progress_along,
    use_scroll_viewport,
};
pub use scroll_viewports::{enclosing_scroll_viewport, scroll_viewports_of, use_primary_scroll};
pub use slots::{Children, SlotRequest, Slots, use_context};
pub use step::{COARSE_STEP, FINE_STEP, step_factor};
pub use styled_container::{
    KeyAnswer, StyledContainer, box_transform, focus_ring_border, style_follows,
};
pub use surface::{Edge, Scrim, SurfaceScaffold, SurfaceTransition};
pub use surface_canvas::{SurfaceCanvas, SurfaceEnv, composite_surface, follow_box_event};
pub use surface_context::Surface;
pub use surface_font::{open_surface_font_family, set_font_family, use_font_family};
pub use surface_frame::SurfaceFrame;
pub use surface_style::{SurfaceAmendment, SurfaceStyle, amend_surface};
pub use surface_title::{
    TitleParts, compose_title, open_surface_title, set_app_title, set_page_title, set_title_format,
    surface_title, use_surface_title,
};
#[cfg(feature = "svg")]
pub use svg::Svg;
pub use text::{Text, TextRun};
pub use text_area::TextArea;
pub use text_edit::DOUBLE_CLICK;
pub use text_editor::{Caret, Underline};
pub use text_input::text_input;
pub use theme_provider::{
    ScopeProvider, follow_control_size, follow_theme, provide_control_size, provide_theme,
};
pub use transform_origin::{TransformOrigin, box_transform_about};
pub use ui_tree::{Component, ComponentList, EventResult, NodeVec, RenderNode};
pub use virtual_list::{VirtualItem, VirtualList, VirtualRows, visible_window};
pub use window_root::WindowRoot;

/// Routes an event to the overlay layer before the widget tree sees it, resolving a live drag first, then Escape/Enter against the dismiss stack, ahead of `ui_tree`'s own pointer routing.
pub fn dispatch_overlays(event: &platform_core::Event) -> EventResult {
    use platform_core::{Event, Key, NamedKey};
    let key = match event {
        Event::KeyPressed {
            key: Key::Named(key),
            ..
        } => Some(key),
        _ => None,
    };
    let cancelled_drag = match event {
        Event::PointerPressed { button, .. } => drag::cancel_live_by(button),
        _ => key == Some(&NamedKey::Escape) && drag::cancel_live(),
    };
    if cancelled_drag {
        return EventResult::Handled;
    }
    // A focused editor gets first refusal on Escape and blurs itself, so a second press closes the dialog: dismissing first would make Escape unable to leave a field without tearing down its form. No other control keeps Escape, so one holding focus — a slider just dragged — does not stand between it and the dialog. Enter goes past focus only where the focused control has no use for it: a button, a field or a spin button acts on it itself.
    let decided = match key {
        Some(NamedKey::Escape) => !focus::text_entry_focused() && dismiss::dismiss_top(),
        Some(NamedKey::Enter) => {
            !focus::focused_keeps(platform_core::ConsumedKeys::ENTER) && dismiss::confirm_top()
        }
        _ => false,
    };
    if decided {
        return EventResult::Handled;
    }
    ui_tree::dispatch_overlays(event)
}
