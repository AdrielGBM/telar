use crate::dev_plugin::DevPlugin;
use platform_core::{Platform, PlatformError};
use renderer_core::RendererFactory;
use services_core::AppPathsProvider;

use crate::app::App;
use crate::app_config::AppConfig;
use crate::config;
use crate::prefs::UserPrefs;

use super::handler::build_app_handler;
use super::host::{SurfaceRenderer, SurfaceWindow};

/// Drive a full rsx app on an arbitrary [`Platform`] backend. This is the backend-agnostic entry point: an
/// out-of-tree backend (e.g. a Wayland layer-shell `Platform`) constructs its own platform and paths provider
/// and calls this, with no winit dependency. [`crate::run_app_with_name`] is the winit-defaulting convenience
/// wrapper over it.
///
/// The whole event → reactive → layout → render → present bridge ([`AppHandler`]) is already generic over the
/// window type, so this simply loads prefs, resolves the renderer backend, applies the app's optional
/// [`App::window_config`] override, and hands a fresh handler to `platform.run`.
///
/// The window has to be a [`SurfaceWindow`]: this entry point draws with the renderers Telar ships, and both need
/// a real surface behind it. A backend whose window is not one uses [`run_with_platform_and_renderer`].
pub fn run_with_platform<P, A, D>(
    platform: P,
    config: AppConfig,
    paths: Box<dyn AppPathsProvider>,
    app: A,
    app_name: &str,
) -> Result<(), PlatformError>
where
    P: Platform,
    P::Window: SurfaceWindow,
    A: App,
    D: DevPlugin,
{
    run_on_platform::<P, A, D>(
        platform,
        config,
        paths,
        app,
        app_name,
        SurfaceRenderer::builtin(),
    )
}

/// The same, drawing through a renderer you install rather than one of Telar's.
///
/// The seam for a frontend Telar knows nothing about: the factory is handed the platform's own window and returns
/// any [`renderer_core::RenderBackend`], which the frame pipeline drives on its own thread exactly as it drives
/// the built-in pair. Nothing here asks the window for a GPU surface, so a terminal backend can implement plain
/// [`platform_core::Window`] and stop there.
///
/// Two consequences: `RendererBackend` and the `telar.backend` preference are ignored, and `AppCtx`'s raw
/// window/display handles come back `None`, since this path never required the window to have any.
pub fn run_with_platform_and_renderer<P, F, A, D>(
    platform: P,
    factory: F,
    config: AppConfig,
    paths: Box<dyn AppPathsProvider>,
    app: A,
    app_name: &str,
) -> Result<(), PlatformError>
where
    P: Platform,
    P::Window: Clone + Send + Sync + 'static,
    F: RendererFactory<P::Window>,
    A: App,
    D: DevPlugin,
{
    run_on_platform::<P, A, D>(
        platform,
        config,
        paths,
        app,
        app_name,
        SurfaceRenderer::installed(factory),
    )
}

fn run_on_platform<P, A, D>(
    platform: P,
    config: AppConfig,
    paths: Box<dyn AppPathsProvider>,
    app: A,
    app_name: &str,
    renderer: SurfaceRenderer<P::Window>,
) -> Result<(), PlatformError>
where
    P: Platform,
    P::Window: Clone + Send + Sync + 'static,
    A: App,
    D: DevPlugin,
{
    let prefs = UserPrefs::load(app_name, paths.as_ref());
    let backend = prefs.backend.unwrap_or_else(config::compile_time_backend);
    let AppConfig {
        window,
        font_paths,
        font_data,
    } = config;
    let window = super::resolved_window(window, &app);
    let handler = build_app_handler::<P::Window, D>(
        Box::new(app),
        paths,
        font_paths,
        font_data,
        backend,
        prefs,
        app_name.to_owned(),
        renderer,
    );
    platform.run(window, handler)
}
