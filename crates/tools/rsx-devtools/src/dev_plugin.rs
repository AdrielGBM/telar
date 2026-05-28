use std::borrow::Cow;
use std::time::Duration;

use platform_core::{Key, ModifiersState};
use renderer_core::DrawCommand;

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

    // Returns true if the press was consumed and should not propagate to the widget tree.
    fn on_pointer_pressed(&mut self, x: f32, y: f32) -> bool;
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
