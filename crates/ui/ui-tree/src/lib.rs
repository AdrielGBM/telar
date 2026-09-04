//! The widget tree: components, the render nodes they emit, and the reactive segments that keep a frame's command list up to date without re-rendering what did not change.

mod component;
mod element;
mod overlay_dispatch;
mod render_node;
mod segment;
mod tree;
mod wheel;

pub use component::{Component, EventResult};
pub use element::{element_capture, set_element_capture};
pub use overlay_dispatch::{
    OverlayContext, OverlayGuard, OverlaySink, dispatch_overlays, register_overlay,
    unregister_overlay,
};
pub use render_node::{NodeVec, RenderNode};
pub use segment::{Segment, SegmentNodeInfo, SegmentRoot};
pub use tree::ComponentList;
pub use wheel::{set_smooth_wheel, smooth_wheel};
