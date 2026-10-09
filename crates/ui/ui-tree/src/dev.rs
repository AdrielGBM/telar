//! The seam an in-app devtools overlay plugs into.
//!
//! A runtime with no overlay compiled in runs `()` through the same trait, so the frame loop has one shape whether or not one is installed. It lives here, beside [`SegmentNodeInfo`], because the tree an inspector reads is what the seam is *about* — and because a crate implementing an overlay should not have to depend on the facade to do it.

use std::borrow::Cow;

use crate::SegmentNodeInfo;
use platform_core::{AccessNode, Event};
use renderer_core::DrawCommand;

/// What a dev overlay asks the runner to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DevAction {
    Redraw,
    ToggleBackend,
}

/// An overlay's answer to one event: whether it kept the event from the tree, and what the runner should do about it.
///
/// Consuming does not redraw by itself; an overlay whose picture changed asks for [`DevAction::Redraw`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OverlayResponse {
    pub consumed: bool,
    pub action: Option<DevAction>,
}

impl OverlayResponse {
    pub const IGNORED: Self = Self {
        consumed: false,
        action: None,
    };
}

/// The dev overlay seam: `()` in a release build, the inspector and FPS counter in a dev one.
pub trait DevOverlay: Default + 'static {
    fn on_frame<'a>(
        &mut self,
        base: &'a [DrawCommand],
        window_w: f32,
        window_h: f32,
        tree_dirty: bool,
    ) -> Cow<'a, [DrawCommand]>;

    /// Sees every event bound for the tree before the tree does; a consumed one never reaches it.
    fn on_event(&mut self, event: &Event) -> OverlayResponse {
        let _ = event;
        OverlayResponse::IGNORED
    }

    /// Whether the overlay wants frames while the app is idle, which the runner then presents at its keepalive cadence.
    fn needs_frame(&self) -> bool {
        false
    }

    /// Whether the overlay's own picture changed since its last [`on_frame`](Self::on_frame), asked before the runner decides whether a frame is due.
    ///
    /// A changed overlay makes that frame one with new content even when the tree beneath it is unchanged: without it a still app re-presents the frame the renderer retained, and the overlay's change waits for the app's next one.
    fn is_dirty(&self) -> bool {
        false
    }

    /// Whether the runner should build an accessibility snapshot after this frame and hand it to [`on_access`](Self::on_access).
    fn wants_access(&self) -> bool {
        false
    }

    /// The window's accessibility nodes as a screen reader would be told them, after a frame for which [`wants_access`](Self::wants_access) said yes.
    fn on_access(&mut self, nodes: &[AccessNode]) {
        let _ = nodes;
    }

    /// Sets (or clears with `None`) the build error banner shown over the app.
    fn set_build_error(&mut self, error: Option<String>) {
        let _ = error;
    }

    /// Names the backend that is now drawing, so the overlay can say which one `ctrl+shift+b` just switched to.
    fn set_renderer_info(&mut self, info: &str) {
        let _ = info;
    }

    /// The mounted component tree as the inspector sees it, once per frame. Takes the walked slice rather than a trait that walks on demand: the two questions the trait offered were asked one after the other, and each walked the whole tree.
    fn on_tree(&mut self, _nodes: &[SegmentNodeInfo]) {}
}

impl DevOverlay for () {
    fn on_frame<'a>(
        &mut self,
        base: &'a [DrawCommand],
        _window_w: f32,
        _window_h: f32,
        _tree_dirty: bool,
    ) -> Cow<'a, [DrawCommand]> {
        Cow::Borrowed(base)
    }
}
