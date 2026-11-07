//! The multi-surface runner: several windows on one UI thread, each with its own surface world.

use std::collections::HashMap;
use std::sync::Arc;

use platform_core::{EventHandler, MultiSurfacePlatform, PlatformError, SurfaceId};
use services_core::AppPathsProvider;

use crate::app::App;
use crate::app_config::AppConfig;

use super::boot::{StoredChoices, SurfaceBoot, split_config};
use super::font_config::FontSetup;

/// Drive **N** independent rsx apps on a [`MultiSurfacePlatform`] backend — one reactive tree per surface, the shape a multi-window app or a desktop shell (a bar/OSD/notification per monitor) needs.
///
/// Each surface `(id, config)` gets a fresh app from `app_factory(id)` and a paths provider from `paths_factory(id)`; the backend builds and drives that surface's handler, on the thread that drives it. Each surface gets a fully isolated reactive/theme/overlay/focus world with no cross-talk. Blocks until all surfaces close.
///
/// The app's directories and preference store are the process's: the first surface built installs them under `app_name`, and every later one, on any thread, shares them.
///
/// This is the multi-surface analogue of [`crate::run_with_platform`]. It always uses the no-op dev overlay (`()`): the per-window devtools overlay is a single-surface concern.
pub fn run_multi_with_platform<P, A, PF, AF>(
    platform: P,
    surfaces: Vec<(SurfaceId, AppConfig)>,
    paths_factory: PF,
    app_factory: AF,
    app_name: &str,
) -> Result<(), PlatformError>
where
    P: MultiSurfacePlatform,
    P::Window: super::host::SurfaceWindow,
    A: App + 'static,
    PF: Fn(SurfaceId) -> Arc<dyn AppPathsProvider> + 'static,
    AF: Fn(SurfaceId) -> A + 'static,
{
    let mut window_configs = Vec::with_capacity(surfaces.len());
    let mut opened: HashMap<SurfaceId, (FontSetup, String)> = HashMap::new();
    for (id, config) in surfaces {
        let (window, fonts) = split_config(config);
        let window = window.titled_or(app_name);
        opened.insert(id, (fonts, window.title.clone()));
        window_configs.push((id, window));
    }
    let app_name = app_name.to_owned();
    platform_core::set_keyboard_convention(platform.keyboard_convention());

    platform.run_surfaces(window_configs, move |id| {
        let (fonts, title) = opened
            .get(&id)
            .cloned()
            .unwrap_or_else(|| (FontSetup::default(), app_name.clone()));
        let boot = SurfaceBoot::new(&app_name, paths_factory(id), fonts, StoredChoices::Now);
        let mut handler = boot.handler::<P::Window, ()>(
            Box::new(crate::app_runtime::LocalApp(app_factory(id))),
            &title,
            super::host::SurfaceRenderer::builtin(),
        );
        // Every surface shares this UI thread and the one reactive runtime, so each needs its own `Surface` world. Built here, on the thread that drives the handler.
        handler.surface = Some(ui_core::Surface::new());
        handler
    })
}

/// Build a driven-ready [`EventHandler`] for a **secondary surface** from an [`AppRuntime`](crate::AppRuntime), boxed so a multi-surface backend can hold it alongside its statically-declared handlers. Mirrors what [`run_multi_with_platform`]'s factory produces — a handler carrying its own `ui_core::Surface` world, booted the same way — but for surfaces opened at runtime (via a `SurfaceHost`), so the backend can enqueue the handler into its single UI-thread loop instead of spawning a thread. Builds no renderer; call it on the thread that drives the handler, whose reactive runtime it brings the app's stored choices back into.
///
/// `config` is the surface's own: its window's title, falling back to `app_name`, and the faces and family it sets text in. The rest of its window is the backend's to apply.
///
/// Takes a runtime rather than an [`App`] because a secondary surface is not always a tree in this process: the dev host opens one for a dylib-backed application, whose runtime is the far side of an FFI boundary. Wrap a plain application in [`LocalApp`](crate::LocalApp).
pub fn build_surface_handler<W, A>(
    app: A,
    paths: Arc<dyn AppPathsProvider>,
    app_name: &str,
    config: AppConfig,
) -> Box<dyn EventHandler<W>>
where
    W: super::host::SurfaceWindow,
    A: crate::app_runtime::AppRuntime,
{
    let (window, fonts) = split_config(config);
    let boot = SurfaceBoot::new(app_name, paths, fonts, StoredChoices::Now);
    let mut handler = boot.handler::<W, ()>(
        Box::new(app),
        &window.titled_or(app_name).title,
        super::host::SurfaceRenderer::builtin(),
    );
    handler.surface = Some(ui_core::Surface::new());
    Box::new(handler)
}
