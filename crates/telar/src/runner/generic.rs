use crate::dev_plugin::DevPlugin;
use platform_core::{Platform, PlatformError};
use services_core::AppPathsProvider;

use crate::app::App;
use crate::app_config::AppConfig;
use crate::config;
use crate::prefs::UserPrefs;

use super::handler::build_app_handler;

/// Drive a full rsx app on an arbitrary [`Platform`] backend. This is the backend-agnostic entry point: an
/// out-of-tree backend (e.g. a Wayland layer-shell `Platform`) constructs its own platform and paths provider
/// and calls this, with no winit dependency. [`crate::run_app_with_name`] is the winit-defaulting convenience
/// wrapper over it.
///
/// The whole event → reactive → layout → render → present bridge ([`AppHandler`]) is already generic over the
/// window type, so this simply loads prefs, resolves the renderer backend, applies the app's optional
/// [`App::window_config`] override, and hands a fresh handler to `platform.run`.
pub fn run_with_platform<P, A, D>(
    platform: P,
    config: AppConfig,
    paths: Box<dyn AppPathsProvider>,
    app: A,
    app_name: &str,
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
    );
    platform.run(window, handler)
}
