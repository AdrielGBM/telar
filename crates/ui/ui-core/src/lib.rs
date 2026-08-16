pub mod accessibility;
#[cfg(feature = "async-assets")]
mod async_asset;
mod border;
mod canvas;
mod child_host;
mod container;
mod context;
pub mod dismiss;
mod drag;
pub mod focus;
mod image;
mod input;
mod input_region;
mod kept;
mod keyboard;
mod layout_item;
mod layout_leaf;
mod lazy;
mod line;
mod line_gutter;
mod named_overlay;
pub mod overlay;
mod path;
mod pointer;
mod press;
mod reactive_list;
mod rect;
mod rich_text;
mod scroll_area;
pub mod scroll_region;
mod slots;
mod styled_container;
mod surface;
mod surface_context;
#[cfg(feature = "svg")]
mod svg;
mod text;
mod text_area;
mod virtual_list;

#[cfg(feature = "async-assets")]
pub use async_asset::{AssetSource, AssetState};
pub use border::{logical_border_radius, logical_border_widths};
pub use canvas::Canvas;
pub use child_host::{
    ChildSlot, fragment, fragment_gap, fragment_positional, fragment_positional_gap,
};
pub use container::Container;
pub use context::{
    NodeId, absolute_rect, compute_layout, compute_layout_root, current_direction, mark_dirty,
    new_container, new_leaf, overlay_viewport, relayout_if_dirty, remove_node,
    reset_layout_runtime, set_children, set_direction, set_display, set_min_height,
    set_overlay_host, track_layout, use_direction,
};
pub use dismiss::{dismiss_depth, dismiss_top, use_dismiss_depth};
pub use drag::{DragStart, drag_start, drag_travel};
pub use image::Image;
pub use input::Input;
pub use input_region::interactive_rects;
pub use kept::kept;
pub use keyboard::{
    end_frame as end_keyboard_frame, key_held, key_pressed, modifiers, observe as observe_keyboard,
    reset as reset_keyboard,
};
pub use layout_item::{ClipAxis, ClippedItem, Holding, LayoutItem, box_item};
pub use lazy::Lazy;
pub use line::Line;
pub use line_gutter::LineGutter;
pub use named_overlay::{close as close_overlay, open as open_overlay, state as overlay_state};
pub use overlay::{Overlay, Placement, anchor_rect};
pub use path::Path;
pub use pointer::{
    PointerButtons, observe_pointer, pointer_buttons, reset_pointer, transform_pointer,
};
pub use reactive_list::ReactiveList;
pub use rect::Rectangle;
pub use rich_text::RichText;
pub use scroll_area::{LayoutScrollArea, ScrollViewport, ScrollbarStyle};
pub use scroll_region::visible_rect;
pub use slots::{Children, Slots, use_context};
pub use styled_container::{StyledContainer, box_transform, style_follows};
pub use surface::{
    DEFAULT_SCRIM, KeyboardMode, MIN_FRAME_SIZE, SurfaceAlign, SurfaceAnchor, SurfaceFrameStyle,
    SurfacePlacement, SurfaceRole, SurfaceRoot, SurfaceScaffold, SurfaceSize, SurfaceTransition,
    surface_frame,
};
pub use surface_context::{Surface, SurfaceGuard};
#[cfg(feature = "svg")]
pub use svg::Svg;
pub use text::Text;
pub use text_area::TextArea;
pub use ui_tree::{Component, ComponentList, EventResult, NodeVec, RenderNode};
pub use virtual_list::{VirtualList, visible_window};

/// Routes an event to the overlay layer before the widget tree sees it.
///
/// Wraps `ui_tree`'s pointer routing with the keyboard half it cannot do: the dismiss stack and the focus
/// state both live in this crate, so Escape is resolved here. Every caller (the runner, the plugin bridge)
/// already goes through `ui_core`, so this is the single choke point either way.
pub fn dispatch_overlays(event: &platform_core::Event) -> EventResult {
    // Escape closes the frontmost dialog only when nothing holds focus: a focused editor gets first refusal
    // and blurs itself (see `Input`'s Escape), so a second press then closes the dialog around it. Dismissing
    // ahead of the focused widget would make Escape unable to leave a field without also tearing down its form.
    if let platform_core::Event::KeyPressed {
        key: platform_core::Key::Named(platform_core::NamedKey::Escape),
        ..
    } = event
        && focus::current().is_none()
        && dismiss::dismiss_top()
    {
        return EventResult::Handled;
    }
    ui_tree::dispatch_overlays(event)
}
