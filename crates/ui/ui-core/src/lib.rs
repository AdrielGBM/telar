#[cfg(feature = "async-assets")]
mod async_asset;
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
mod layout_item;
mod layout_leaf;
mod line;
mod line_gutter;
pub mod overlay;
mod path;
mod pointer;
mod press;
mod reactive_list;
mod rect;
mod rich_text;
mod scroll_area;
mod slots;
mod styled_container;
mod surface;
mod surface_context;
#[cfg(feature = "svg")]
mod svg;
mod text;
mod text_area;

#[cfg(feature = "async-assets")]
pub use async_asset::{AssetSource, AssetState};
pub use canvas::Canvas;
pub use child_host::{
    ChildSlot, fragment, fragment_gap, fragment_positional, fragment_positional_gap,
};
pub use container::Container;
pub use context::{
    NodeId, absolute_rect, compute_layout, compute_layout_root, mark_dirty, new_container,
    new_leaf, relayout_if_dirty, remove_node, reset_layout_runtime, set_children, set_display,
    set_min_height, set_overlay_host, track_layout,
};
pub use dismiss::{dismiss_depth, dismiss_top, use_dismiss_depth};
pub use image::Image;
pub use input::Input;
pub use input_region::{interactive_rects, register_interactive, unregister_interactive};
pub use layout_item::{ClippedItem, LayoutItem, box_item};
pub use layout_leaf::LayoutLeaf;
pub use line::Line;
pub use line_gutter::LineGutter;
pub use overlay::{Overlay, Placement, anchor_rect};
pub use path::Path;
pub use reactive_list::ReactiveList;
pub use rect::Rectangle;
pub use rich_text::RichText;
pub use scroll_area::{LayoutScrollArea, ScrollViewport, ScrollbarStyle};
pub use slots::Slots;
pub use styled_container::{StyledContainer, box_transform};
pub use surface::{
    DEFAULT_SCRIM, SurfaceAlign, SurfaceAnchor, SurfaceFrameStyle, SurfacePlacement, SurfaceRole,
    SurfaceRoot, SurfaceScaffold, SurfaceSize, surface_frame,
};
pub use surface_context::{Surface, SurfaceGuard};
#[cfg(feature = "svg")]
pub use svg::Svg;
pub use text::Text;
pub use text_area::TextArea;
pub use ui_tree::{Component, ComponentList, EventResult, NodeVec, RenderNode};

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
