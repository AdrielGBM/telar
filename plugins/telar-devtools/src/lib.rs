//! The dev overlay `cargo telar dev` draws over a running application: an FPS badge and panel, the renderer in use, the inspection panels and the build-error banner.
//!
//! The inspection panels are public: an [`Inspection`] reads an [`InspectSource`] — the whole window under the overlay, or a canvas a workshop shows — and [`inspector_panel`], [`accessibility_panel`], [`inspection_toggles`] and [`inspection_marks`] show it, in the overlay and in the workshop alike.
//!
//! The overlay is a widget tree built from `telar-components` in the workbench theme, on a window-sized [`SurfaceCanvas`](telar::SurfaceCanvas) of its own in the host's runtime. The host owns it, so it outlives every hot reload and every failed build, and it never reads the application's theme, direction or control size.
//!
//! Nothing here is privileged. It implements [`telar::DevOverlay`], which is the whole of what the runner asks of an overlay, and an overlay of your own goes in through the same door — see `telar::run_app_with_devtools`.
//!
//! Its text is in English under the `telar_devtools` namespace, which an application translates or overrides from its own catalog with `telar_devtools.<key>`.
#![warn(rustdoc::broken_intra_doc_links)]

mod inspect;
mod meter;
mod overlay;
mod panels;
mod strings;
mod workbench;
pub use inspect::{InspectSource, InspectedNode, Inspection, WindowSource};
pub use panels::{accessibility_panel, inspection_marks, inspection_toggles, inspector_panel};
pub use workbench::{
    WORKBENCH_CONTROL_SIZE, WORKBENCH_GRID, WORKBENCH_RADIUS, WORKBENCH_TEXT_SIZE, WorkbenchTheme,
    WorkbenchTokens, use_workbench_tokens, workbench_card, workbench_fill, workbench_mono,
    workbench_muted, workbench_scope, workbench_theme,
};

use std::borrow::Cow;

use telar::{
    AccessNode, AccessViolation, Chord, DevAction, DevOverlay, DrawCommand, Event, Key,
    ModifiersState, OverlayResponse, SegmentNodeInfo, Size,
};

use meter::{Clock, FrameMeter};
use overlay::Overlay;

/// The chords the overlay answers, wherever the application runs: Ctrl rather than the primary modifier, so they never meet a Command shortcut of the application or the browser.
pub(crate) const SWITCH_RENDERER_CHORD: &str = "Ctrl+Shift+B";
pub(crate) const SHOW_PANEL_CHORD: &str = "Ctrl+Shift+D";
pub(crate) const SHOW_INSPECTOR_CHORD: &str = "Ctrl+Shift+I";

/// The dev overlay: an FPS badge and panel, the inspection panels and the build-error banner.
///
/// It takes the pointer only over its own panels and the keyboard only while one of them holds focus; everything else reaches the application. Ctrl+Shift+B switches the renderer, Ctrl+Shift+D opens the panel and Ctrl+Shift+I the inspector, and the application hears those chords too.
#[derive(Default)]
pub struct DevTools {
    meter: FrameMeter,
    clock: Clock,
    overlay: Built,
    window: WindowSource,
}

/// The widget tree is built on first use rather than in `Default`, which the runner may call on another thread than the one that drives it.
#[derive(Default)]
enum Built {
    #[default]
    Pending,
    Ready(Box<Overlay>),
    Failed,
}

impl Built {
    fn ready(&mut self) -> Option<&mut Overlay> {
        if matches!(self, Built::Pending) {
            *self = match Overlay::new() {
                Ok(overlay) => Built::Ready(Box::new(overlay)),
                Err(error) => {
                    tracing::error!("the devtools overlay could not be built: {error}");
                    Built::Failed
                }
            };
        }
        match self {
            Built::Ready(overlay) => Some(overlay),
            _ => None,
        }
    }
}

impl DevTools {
    fn overlay(&mut self) -> Option<&mut Overlay> {
        self.overlay.ready()
    }

    fn show_window(&mut self) {
        if let Some(overlay) = self.overlay.ready() {
            overlay.refresh(&self.window);
        }
    }

    fn shortcut(&mut self, key: &Key, modifiers: ModifiersState) -> Option<DevAction> {
        let pressed =
            |chord: &str| Chord::parse(chord).is_ok_and(|chord| chord.matches(key, modifiers));
        if pressed(SWITCH_RENDERER_CHORD) {
            Some(DevAction::ToggleBackend)
        } else if pressed(SHOW_PANEL_CHORD) {
            self.overlay()?.toggle_panel();
            Some(DevAction::Redraw)
        } else if pressed(SHOW_INSPECTOR_CHORD) {
            self.overlay()?.toggle_inspector();
            Some(DevAction::Redraw)
        } else {
            None
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
        let Some(overlay) = self.overlay.ready() else {
            return Cow::Borrowed(base);
        };
        overlay.show_reading(reading);
        overlay.fit(Size::new(window_w, window_h));
        let drawn = overlay.frame();
        let mut commands = Vec::with_capacity(base.len() + drawn.len() + 2);
        commands.extend_from_slice(base);
        commands.push(DrawCommand::PushSurface {
            surface: overlay.surface(),
        });
        commands.extend_from_slice(&drawn);
        commands.push(DrawCommand::PopSurface);
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

    fn set_app_panic(&mut self, panic: Option<String>) {
        if let Some(overlay) = self.overlay() {
            overlay.set_app_panic(panic);
        }
    }

    fn set_renderer_info(&mut self, info: &str) {
        if let Some(overlay) = self.overlay() {
            overlay.set_renderer(info);
        }
    }

    fn on_tree(&mut self, nodes: &[SegmentNodeInfo]) {
        self.window.set_nodes(nodes);
        self.show_window();
    }

    fn forget_frames(&mut self) {
        self.window.set_nodes(&[]);
    }

    fn wants_access(&self) -> bool {
        match &self.overlay {
            Built::Ready(overlay) => overlay.wants_audit(),
            _ => false,
        }
    }

    fn on_access(&mut self, nodes: &[AccessNode]) {
        self.window.set_access(nodes);
        self.show_window();
    }

    fn on_violations(&mut self, violations: &[AccessViolation]) {
        self.window.set_violations(violations);
        self.show_window();
    }
}

#[cfg(test)]
#[path = "lib_test.rs"]
mod tests;
