use std::collections::HashMap;
use std::sync::Arc;

use platform_core::{EventHandler, MultiSurfacePlatform, PlatformError, SurfaceId};
use services_core::AppPathsProvider;

use crate::app::App;
use crate::app_config::AppConfig;
use crate::config;
use crate::prefs::UserPrefs;

use super::handler::build_app_handler;

/// One surface's fonts, split out of its [`AppConfig`] until its handler is built: the faces to load, and the
/// family among them its text shapes in.
type SurfaceFonts = (Vec<std::path::PathBuf>, Vec<Vec<u8>>, Option<String>);

/// Drive **N** independent rsx apps on a [`MultiSurfacePlatform`] backend — one reactive tree per surface, the
/// shape a multi-window app or a desktop shell (a bar/OSD/notification per monitor) needs.
///
/// Each surface `(id, config)` gets a fresh app from `app_factory(id)` and a paths provider from
/// `paths_factory(id)`; the backend builds and drives that surface's handler. On the headless and out-of-tree
/// backends each surface runs on its own thread, so it gets a fully isolated reactive/theme/overlay/focus world
/// with no cross-talk. Blocks until all surfaces close.
///
/// This is the multi-surface analogue of [`crate::run_with_platform`]. It always uses the no-op dev plugin
/// (`()`): the per-window devtools overlay is a single-surface concern.
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
    // Split each surface's AppConfig into the WindowConfig the platform needs and the fonts the handler factory
    // needs (keyed by SurfaceId, shared read-only across the surface threads).
    let mut window_configs = Vec::with_capacity(surfaces.len());
    let mut fonts: HashMap<SurfaceId, SurfaceFonts> = HashMap::new();
    for (id, cfg) in surfaces {
        let AppConfig {
            window,
            font_paths,
            font_data,
            font_family,
        } = cfg;
        window_configs.push((id, window));
        fonts.insert(id, (font_paths, font_data, font_family));
    }
    let fonts = Arc::new(fonts);
    let app_name = app_name.to_owned();

    platform.run_surfaces(window_configs, move |id| {
        let app = app_factory(id);
        let paths = paths_factory(id);
        let prefs = UserPrefs::load(&app_name, paths.as_ref());
        let backend = prefs.backend.unwrap_or_else(config::compile_time_backend);
        let (font_paths, font_data, font_family) = fonts.get(&id).cloned().unwrap_or_default();
        let mut handler = build_app_handler::<P::Window, ()>(
            Box::new(app),
            paths,
            font_paths,
            font_data,
            font_family,
            backend,
            prefs,
            app_name.clone(),
            super::host::SurfaceRenderer::builtin(),
        );
        // Single-thread multi-surface (M3): every surface shares this UI thread and the one reactive runtime,
        // so each needs its own `Surface` world (layout/overlay/focus/...). The handler activates it around
        // each lifecycle call (`AppHandler::enter_surface`). Built here, on the thread that drives the handler.
        handler.surface = Some(ui_core::Surface::new());
        handler
    })
}

/// Build a driven-ready [`EventHandler`] for a **secondary surface** from an [`App`], boxed so a multi-surface
/// backend can hold it alongside its statically-declared handlers. Mirrors what [`run_multi_with_platform`]'s
/// factory produces — a handler carrying its own `ui_core::Surface` world — but for surfaces opened at runtime
/// (via a `SurfaceHost`), so the backend can enqueue the handler into its single UI-thread loop instead of
/// spawning a thread. Builds no renderer and touches no thread-local reactive state; the loop drives it.
pub fn build_surface_handler<W, A>(
    app: A,
    paths: Arc<dyn AppPathsProvider>,
    app_name: &str,
    fonts: AppConfig,
) -> Box<dyn EventHandler<W>>
where
    W: super::host::SurfaceWindow,
    A: App + 'static,
{
    let prefs = UserPrefs::load(app_name, paths.as_ref());
    let backend = prefs.backend.unwrap_or_else(config::compile_time_backend);
    // A surface opened at runtime carries its own font configuration like any other. It used to be handed
    // none at all, which was survivable only while a process-wide global named the family behind its back.
    let AppConfig {
        window: _,
        font_paths,
        font_data,
        font_family,
    } = fonts;
    let mut handler = build_app_handler::<W, ()>(
        Box::new(app),
        paths,
        font_paths,
        font_data,
        font_family,
        backend,
        prefs,
        app_name.to_string(),
        super::host::SurfaceRenderer::builtin(),
    );
    handler.surface = Some(ui_core::Surface::new());
    Box::new(handler)
}
