//! Where a canvas takes its environment from: the toolbar's [`CanvasSettings`](crate::settings::CanvasSettings) over the preview's own [`telar::preview::PreviewEnv`].

use std::rc::Rc;

use telar::preview::{Globals, Layout, Matrices, PreviewEntry, PreviewEnv};
use telar::{
    LayoutError, LayoutItem, NestedCanvas, RwSignal, Size, effect, set_reduced_motion_override,
    use_resolved_scheme,
};

use super::frame::clamp_viewport;
use crate::settings::CanvasSize;
use crate::state::WorkshopState;

/// Builds `build`'s content on a canvas in the environment the settings and `entry` resolve to, and keeps it there: the mode, direction, locale, high contrast, viewport, a device's safe area and the background, through `globals`. A custom size is the one the handles set held within `custom_room`, so a stage that shrinks resizes the canvas rather than zooming it out.
pub(super) fn canvas(
    state: &WorkshopState,
    entry: &PreviewEntry,
    globals: Globals,
    custom_room: RwSignal<Option<Size>>,
    build: impl FnOnce() -> Result<Box<dyn LayoutItem>, LayoutError>,
) -> Result<Rc<NestedCanvas>, LayoutError> {
    resolved_canvas(
        state,
        entry.env,
        entry.layout != Layout::Fullscreen,
        globals,
        Some(custom_room),
        build,
    )
}

/// [`canvas`] for a canvas its page sizes, as the Docs page does: the toolbar's viewport and device leave it alone.
pub(crate) fn canvas_unframed(
    state: &WorkshopState,
    env: &PreviewEnv,
    globals: Globals,
    build: impl FnOnce() -> Result<Box<dyn LayoutItem>, LayoutError>,
) -> Result<Rc<NestedCanvas>, LayoutError> {
    resolved_canvas(state, *env, false, globals, None, build)
}

fn resolved_canvas(
    state: &WorkshopState,
    env: PreviewEnv,
    framed: bool,
    globals: Globals,
    custom_room: Option<RwSignal<Option<Size>>>,
    build: impl FnOnce() -> Result<Box<dyn LayoutItem>, LayoutError>,
) -> Result<Rc<NestedCanvas>, LayoutError> {
    let settings = state.canvas_settings();
    let project = Matrices::installed().viewports();
    effect(move || {
        let scheme = use_resolved_scheme();
        let settings = settings.get();
        let resolved = settings.resolve(&env, framed, project, scheme);
        let room = custom_room.and_then(|room| room.get());
        let viewport = match (&settings.size, resolved.viewport, room) {
            (CanvasSize::Custom { .. }, Some(size), Some(room)) => Some(clamp_viewport(size, room)),
            _ => resolved.viewport,
        };
        globals.mode().set_if_changed(resolved.mode);
        globals.locale().set_if_changed(resolved.locale);
        globals.direction().set_if_changed(resolved.direction);
        globals
            .high_contrast()
            .set_if_changed(resolved.high_contrast);
        globals.background().set_if_changed(resolved.background);
        globals.viewport().set_if_changed(viewport);
        globals.safe_area().set_if_changed(resolved.safe_area);
    });
    globals.canvas(Size::ZERO, build)
}

/// Keeps the application's reduced-motion override as the toolbar sets it, whatever the workshop shows: motion has one switch for the whole process, so this reaches the workshop's chrome too.
pub(crate) fn follow_reduced_motion(state: &WorkshopState) {
    let settings = state.canvas_settings();
    effect(move || {
        let reduced = settings.with(|settings| settings.reduced_motion);
        set_reduced_motion_override(reduced.then_some(true));
    });
}
