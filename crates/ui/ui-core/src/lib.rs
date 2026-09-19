//! The widget catalogue's foundation: the primitives every component is built from, and the ambient state they share — focus, pointer, overlays, the cascade and the per-surface worlds all of it lives in.

#![warn(rustdoc::broken_intra_doc_links)]

pub mod accessibility;
#[cfg(feature = "async-assets")]
mod async_asset;
mod border;
mod canvas;
mod caret;
mod child_host;
mod container;
mod context;
mod cursor;
pub mod dismiss;
mod disposal;
mod drag;
mod element;
mod error_boundary;
mod fling;
pub mod focus;
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
mod line_gutter;
mod named_overlay;
pub mod overlay;
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
mod serial;
mod slots;
mod step;
mod styled_container;
mod surface;
mod surface_context;
#[cfg(feature = "svg")]
mod svg;
#[cfg(test)]
mod test_support;
mod text;
mod text_area;
mod text_metrics;
mod theme_provider;
pub use text_metrics::{SINGLE_LINE_LEADING, single_line_box};
mod virtual_list;
mod window_root;

#[cfg(feature = "async-assets")]
pub use async_asset::{
    AssetCache, AssetDecoder, AssetError, AssetKey, AssetLoader, AssetState, AssetTransport, Reply,
};
pub use border::{logical_border_radius, logical_border_widths};
pub use canvas::Canvas;
pub use child_host::{ChildSlot, fragment, fragment_positional};
pub use container::Container;
pub use context::{
    NodeId, absolute_rect, compute_layout, current_direction, live_node_count, mark_dirty,
    new_container, new_leaf, overlay_viewport, relayout_if_dirty, remove_node,
    reset_layout_runtime, set_children, set_direction, set_display, set_min_height,
    set_overlay_host, track_layout, use_direction,
};
pub use cursor::requested_cursor;
pub use dismiss::{
    confirm_top, dismiss_depth, dismiss_top, register_transaction, use_dismiss_depth,
};
pub use drag::{DragAxis, DragStart, drag_start, drag_travel};
pub use error_boundary::{BuildFailure, ErrorBoundary};
pub use image::Image;
pub use inherit::{Inherited, context, declare, inherited_text_style, undeclare};
pub use input::Input;
pub use input_region::{interactive_rects, visible_rect};
pub use kept::kept;
pub use keyboard::{
    end_frame as end_keyboard_frame, key_held, key_pressed, modifiers, observe as observe_keyboard,
    reset as reset_keyboard,
};
pub use keynav::{KeyNav, KeyNavMove, key_nav_apply, key_nav_apply_grid};
pub use layout_item::{Clip, ClipAxis, ClipPointer, ClippedItem, LayoutItem, box_item};
pub use layout_transition::{LayoutTransition, animate_layout};
pub use lazy::Lazy;
pub use line_gutter::LineGutter;
pub use named_overlay::{close as close_overlay, open as open_overlay, state as overlay_state};
pub use overlay::{Overlay, Placement, anchor_rect};
pub use path::Path;
pub use pointer::{
    PointerButtons, observe_pointer, pointer_buttons, reset_pointer, transform_pointer,
};
pub use presence::{Presence, Transition, exits_in_flight};
pub use reactive_list::ReactiveList;
pub use rect::Rectangle;
pub use reorder::{Axis, apply_move, insertion_index};
pub use scroll_area::{LayoutScrollArea, ScrollViewport, ScrollbarStyle};
pub use scroll_page::ScrollPage;
pub use slots::{Children, Slots, use_context};
pub use step::{COARSE_STEP, FINE_STEP, step_factor};
pub use styled_container::{KeyAnswer, StyledContainer, box_transform, style_follows};
pub use surface::{DEFAULT_SCRIM, Edge, SurfaceScaffold, SurfaceTransition};
pub use surface_context::{Surface, SurfaceGuard};
#[cfg(feature = "svg")]
pub use svg::Svg;
pub use text::Text;
pub use text_area::TextArea;
pub use theme_provider::{ThemeProvider, provide_theme};
pub use ui_tree::{Component, ComponentList, EventResult, NodeVec, RenderNode};
pub use virtual_list::{VirtualList, visible_window};
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
    // Only when nothing holds focus: a focused editor gets first refusal and blurs itself, so a second press closes the dialog. Dismissing first would make Escape unable to leave a field without tearing down its form.
    let unfocused = || focus::current().is_none();
    let decided = match key {
        Some(NamedKey::Escape) => unfocused() && dismiss::dismiss_top(),
        Some(NamedKey::Enter) => unfocused() && dismiss::confirm_top(),
        _ => false,
    };
    if decided {
        return EventResult::Handled;
    }
    ui_tree::dispatch_overlays(event)
}
