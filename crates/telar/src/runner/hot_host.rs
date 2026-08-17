//! The `cargo telar dev` host: a binary that loads the app from a dylib and reloads it in place.

use std::sync::Arc;

use platform_core::Platform;
use platform_desktop::{DesktopPathsProvider, WinitPlatform, WinitWindow};

use crate::app::App;
use crate::config;
use crate::prefs::UserPrefs;

use super::handler::build_app_handler;

pub fn run_hot_reload_host(
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
    platform_desktop::DesktopFileDialogs::install();
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
        font_paths,
        font_data,
    } = config;
    #[cfg(telar_hot_reload)]
    super::desktop::apply_dev_window_overrides(&mut window);
    if let Some(custom) = initial_app.window_config() {
        window = custom;
    }
    // Share the one field literal with `run_with_platform` (via build_app_handler); only the hot-reload
    // receiver differs from a normal single-window handler.
    let mut handler = build_app_handler::<WinitWindow, crate::dev_tools::DevTools>(
        Box::new(initial_app),
        paths,
        font_paths,
        font_data,
        backend,
        prefs,
        app_name.to_owned(),
        super::host::SurfaceRenderer::builtin(),
    );
    handler.hot_reload_rx = Some(hot_rx);
    if let Err(e) = platform.run(window, handler) {
        tracing::error!("Event loop error: {e}");
        std::process::exit(1);
    }
}
