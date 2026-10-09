//! The dev overlay `cargo telar dev` draws over a running application: an FPS badge and panel, the renderer in use, a component inspector and the build-error banner.
//!
//! The overlay is a widget tree built from `telar-components` in the workbench theme, on a window-sized [`SurfaceCanvas`](telar::SurfaceCanvas) of its own in the host's runtime. The host owns it, so it outlives every hot reload and every failed build, and it never reads the application's theme, direction or control size.
//!
//! Nothing here is privileged. It implements [`telar::DevOverlay`], which is the whole of what the runner asks of an overlay, and an overlay of your own goes in through the same door — see `telar::run_app_with_devtools`.
//!
//! Its text is in English under the `telar_devtools` namespace, which an application translates or overrides from its own catalog with `telar_devtools.<key>`.
#![warn(rustdoc::broken_intra_doc_links)]

mod meter;
mod overlay;
mod strings;
mod workbench;
pub use workbench::{
    WORKBENCH_CONTROL_SIZE, WORKBENCH_GRID, WORKBENCH_RADIUS, WORKBENCH_TEXT_SIZE, WorkbenchTheme,
    WorkbenchTokens, use_workbench_tokens, workbench_card, workbench_fill, workbench_mono,
    workbench_muted, workbench_scope, workbench_theme,
};

use std::borrow::Cow;

use telar::{
    DevAction, DevOverlay, DrawCommand, Event, Key, ModifiersState, OverlayResponse,
    SegmentNodeInfo, Size,
};

use meter::{Clock, FrameMeter};
use overlay::Overlay;

/// The dev overlay: an FPS badge and panel, a component inspector and the build-error banner.
///
/// It takes the pointer only over its own panels and the keyboard only while one of them holds focus; everything else reaches the application. Ctrl+Shift+B switches the renderer, Ctrl+Shift+D opens the panel and Ctrl+Shift+I the inspector, and the application hears those chords too.
#[derive(Default)]
pub struct DevTools {
    meter: FrameMeter,
    clock: Clock,
    overlay: Built,
}

/// The widget tree is built on first use rather than in `Default`, which the runner may call on another thread than the one that drives it.
#[derive(Default)]
enum Built {
    #[default]
    Pending,
    Ready(Box<Overlay>),
    Failed,
}

impl DevTools {
    fn overlay(&mut self) -> Option<&mut Overlay> {
        if matches!(self.overlay, Built::Pending) {
            self.overlay = match Overlay::new() {
                Ok(overlay) => Built::Ready(Box::new(overlay)),
                Err(error) => {
                    tracing::error!("the devtools overlay could not be built: {error}");
                    Built::Failed
                }
            };
        }
        match &mut self.overlay {
            Built::Ready(overlay) => Some(overlay),
            _ => None,
        }
    }

    fn shortcut(&mut self, key: &Key, modifiers: ModifiersState) -> Option<DevAction> {
        if !(modifiers.is_ctrl && modifiers.is_shift) {
            return None;
        }
        match key {
            Key::Char('b' | 'B') => Some(DevAction::ToggleBackend),
            Key::Char('d' | 'D') => {
                self.overlay()?.toggle_panel();
                Some(DevAction::Redraw)
            }
            Key::Char('i' | 'I') => {
                self.overlay()?.toggle_inspector();
                Some(DevAction::Redraw)
            }
            _ => None,
        }
    }
}

impl DevOverlay for DevTools {
    fn on_frame<'a>(
        &mut self,
        base: &'a [DrawCommand],
        window_w: f32,
        window_h: f32,
        tree_dirty: bool,
    ) -> Cow<'a, [DrawCommand]> {
        let reading = self.meter.sample(self.clock.now(), tree_dirty);
        let Some(overlay) = self.overlay() else {
            return Cow::Borrowed(base);
        };
        overlay.show_reading(reading);
        overlay.fit(Size::new(window_w, window_h));
        let drawn = overlay.frame();
        let mut commands = Vec::with_capacity(base.len() + drawn.len());
        commands.extend_from_slice(base);
        commands.extend_from_slice(&drawn);
        Cow::Owned(commands)
    }

    // The FPS badge falls to zero only if frames keep coming while the app is idle.
    fn needs_frame(&self) -> bool {
        true
    }

    fn is_dirty(&self) -> bool {
        match &self.overlay {
            Built::Ready(overlay) => overlay.is_dirty() || self.meter.is_stale(self.clock.now()),
            _ => false,
        }
    }

    fn on_event(&mut self, event: &Event) -> OverlayResponse {
        if let Event::KeyPressed { key, modifiers, .. } = event
            && let Some(action) = self.shortcut(key, *modifiers)
        {
            return OverlayResponse {
                consumed: false,
                action: Some(action),
            };
        }
        match self.overlay() {
            Some(overlay) => overlay.route(event),
            None => OverlayResponse::IGNORED,
        }
    }

    fn set_build_error(&mut self, error: Option<String>) {
        if let Some(overlay) = self.overlay() {
            overlay.set_build_error(error);
        }
    }

    fn set_renderer_info(&mut self, info: &str) {
        if let Some(overlay) = self.overlay() {
            overlay.set_renderer(info);
        }
    }

    fn on_tree(&mut self, nodes: &[SegmentNodeInfo]) {
        if let Some(overlay) = self.overlay() {
            overlay.set_nodes(nodes);
        }
    }
}

#[cfg(test)]
#[path = "lib_test.rs"]
mod tests;
