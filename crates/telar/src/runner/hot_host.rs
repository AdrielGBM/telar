//! The `cargo telar dev` host: a binary that loads the app from a dylib and reloads it in place.

use std::sync::Arc;

use platform_core::Platform;
use platform_desktop::{DesktopPathsProvider, WinitPlatform, WinitWindow};

use crate::app_runtime::AppRuntime;
use crate::config;
use crate::prefs::UserPrefs;

use super::handler::build_app_handler;

/// Runs the dev host: a window whose application comes from a dylib and is swapped on each rebuild, with `D` drawn over it.
///
/// `D` lives in the host and survives every swap. `telar::app!` names `telar-devtools` here when the application declares it, and `()` otherwise. `TELAR_DEVTOOLS=0` installs no overlay, whichever `D` is.
pub fn run_hot_reload_host<D: ui_tree::DevOverlay>(
    lib_path: &str,
    hot_port: &str,
    config: crate::app_config::AppConfig,
    app_name: &str,
) {
    if super::overlay_disabled() {
        run_hot_reload_host_with::<()>(lib_path, hot_port, config, app_name);
    } else {
        run_hot_reload_host_with::<D>(lib_path, hot_port, config, app_name);
    }
}

fn run_hot_reload_host_with<D: ui_tree::DevOverlay>(
    lib_path: &str,
    hot_port: &str,
    config: crate::app_config::AppConfig,
    app_name: &str,
) {
    let Ok(port) = hot_port.parse::<u16>() else {
        tracing::error!("invalid TELAR_HOT_PORT value: {hot_port}");
        std::process::exit(1);
    };
    let initial_app = match crate::hot::load_hot_app(std::path::Path::new(lib_path)) {
        Ok(app) => app,
        Err(e) => {
            tracing::error!("failed to load dylib: {e}");
            std::process::exit(1);
        }
    };
    let hot_rx = crate::hot::listen_hot_reload(port);
    let paths: Arc<dyn services_core::AppPathsProvider> = Arc::new(DesktopPathsProvider);
    #[cfg(feature = "dialogs")]
    platform_desktop::DesktopFileDialogs::install();
    #[cfg(feature = "clipboard")]
    platform_desktop::DesktopClipboard::install();
    platform_desktop::DesktopUriOpener::install();
    let prefs = UserPrefs::load(app_name, paths.as_ref());
    let backend = prefs.backend.unwrap_or_else(config::compile_time_backend);
    let platform = match WinitPlatform::try_new() {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Failed to create event loop: {e}");
            std::process::exit(1);
        }
    };
    let crate::app_config::AppConfig {
        mut window,
        fonts: faces,
        font_family,
    } = config;
    #[cfg(feature = "hot-reload")]
    super::dev_window::apply_dev_window_overrides(&mut window);
    if let Some(custom) = initial_app.window_config() {
        window = custom;
    }
    // Share the one field literal with `run_with_platform` (via build_app_handler); only the hot-reload receiver differs from a normal single-window handler.
    let mut handler = build_app_handler::<WinitWindow, D>(
        Box::new(initial_app),
        paths,
        crate::runner::font_config::FontSetup {
            faces,
            family: font_family,
        },
        backend,
        prefs,
        app_name.to_owned(),
        super::host::SurfaceRenderer::builtin(),
    );
    handler.hot_reload_rx = Some(hot_rx);
    handler.title = Some(super::state::WindowTitle::opened_as(&window.title));
    handler.location = Some(super::location::LocationBinding::remembered(Box::new(
        platform_core::ArgumentLocation::from_env(),
    )));
    if let Err(e) = platform.run(window, handler) {
        tracing::error!("Event loop error: {e}");
        std::process::exit(1);
    }
}
