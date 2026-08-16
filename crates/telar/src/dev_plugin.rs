//! The seam the in-app devtools overlay plugs into, and the tree model it reads.
//!
//! A runtime with no overlay compiled in runs `()` through the same trait, so the frame loop has one shape
//! whether or not `dev` is on.

use std::borrow::Cow;
use std::time::Duration;

use platform_core::{Key, ModifiersState};
use renderer_core::DrawCommand;
use ui_tree::SegmentNodeInfo;

pub enum DevAction {
    None,
    Redraw,
    ToggleBackend,
}

pub trait DevPlugin: Default + 'static {
    fn on_frame<'a>(
        &mut self,
        base: &'a [DrawCommand],
        window_w: f32,
        window_h: f32,
        tree_dirty: bool,
    ) -> Cow<'a, [DrawCommand]>;

    fn keepalive_interval(&self) -> Option<Duration>;

    fn on_key(&mut self, key: &Key, modifiers: ModifiersState) -> DevAction;

    fn on_pointer_pressed(&mut self, x: f32, y: f32) -> bool;

    /// Sets (or clears with `None`) the build error banner shown over the app.
    fn set_build_error(&mut self, error: Option<String>) {
        let _ = error;
    }

    /// The mounted component tree as the inspector sees it, once per frame. Takes the walked slice rather
    /// than a trait that walks on demand: the two questions the trait offered were asked one after the
    /// other, and each walked the whole tree.
    fn on_tree(&mut self, _nodes: &[SegmentNodeInfo]) {}
}

impl DevPlugin for () {
    fn on_frame<'a>(
        &mut self,
        base: &'a [DrawCommand],
        _window_w: f32,
        _window_h: f32,
        _tree_dirty: bool,
    ) -> Cow<'a, [DrawCommand]> {
        Cow::Borrowed(base)
    }

    fn keepalive_interval(&self) -> Option<Duration> {
        None
    }

    fn on_key(&mut self, _key: &Key, _modifiers: ModifiersState) -> DevAction {
        DevAction::None
    }

    fn on_pointer_pressed(&mut self, _x: f32, _y: f32) -> bool {
        false
    }
}
