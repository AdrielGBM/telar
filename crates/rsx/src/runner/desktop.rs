#[cfg(not(target_os = "android"))]
use devtools_core::DevPlugin;
#[cfg(not(target_os = "android"))]
use platform_core::Platform;
#[cfg(not(target_os = "android"))]
use platform_desktop::{WinitPlatform, WinitWindow};
#[cfg(not(target_os = "android"))]
use services_core::AppPathsProvider;

#[cfg(not(target_os = "android"))]
use crate::app::App;
#[cfg(not(target_os = "android"))]
use crate::app_config::AppConfig;
#[cfg(not(target_os = "android"))]
use crate::config;
#[cfg(not(target_os = "android"))]
use crate::prefs::UserPrefs;
#[cfg(not(target_os = "android"))]
use platform_desktop::DesktopPathsProvider;

#[cfg(not(target_os = "android"))]
use super::handler::AppHandler;

#[cfg(rsx_hot_reload)]
pub(super) fn apply_dev_window_overrides(config: &mut platform_core::WindowConfig) {
    if let Ok(v) = std::env::var("RSX_DEV_WINDOW_TITLE") {
        config.title = v;
    }
    if let Ok(v) = std::env::var("RSX_DEV_WINDOW_WIDTH") {
        if let Ok(n) = v.parse() {
            config.width = n;
        }
    }
    if let Ok(v) = std::env::var("RSX_DEV_WINDOW_HEIGHT") {
        if let Ok(n) = v.parse() {
            config.height = n;
        }
    }
    if let Ok(v) = std::env::var("RSX_DEV_WINDOW_DECORATIONS") {
        config.has_decorations = v == "1";
    }
    if let Ok(v) = std::env::var("RSX_DEV_WINDOW_RESIZABLE") {
        config.is_resizable = v == "1";
    }
    if let Ok(v) = std::env::var("RSX_DEV_WINDOW_TRANSPARENT") {
        config.is_transparent = v == "1";
    }
    if let Ok(v) = std::env::var("RSX_DEV_WINDOW_FULLSCREEN") {
        config.fullscreen = match v.as_str() {
            "borderless" => platform_core::FullscreenMode::Borderless,
            "exclusive" => platform_core::FullscreenMode::Exclusive,
            _ => platform_core::FullscreenMode::Disabled,
        };
    }
    if let Ok(v) = std::env::var("RSX_DEV_WINDOW_POSITION") {
        config.position = parse_dev_window_position(&v);
    }
}

// Parses the RSX_DEV_WINDOW_POSITION value: "centered" (or empty/invalid) → Centered; "<x>,<y>" → absolute coordinates.
#[cfg(rsx_hot_reload)]
fn parse_dev_window_position(value: &str) -> platform_core::WindowPosition {
    let value = value.trim();
    if let Some((x, y)) = value.split_once(',')
        && let (Ok(x), Ok(y)) = (x.trim().parse::<i32>(), y.trim().parse::<i32>())
    {
        return platform_core::WindowPosition::At(x, y);
    }
    platform_core::WindowPosition::Centered
}

#[cfg(not(target_os = "android"))]
fn run_desktop_with_plugin<A: App, D: DevPlugin>(config: AppConfig, app: A, app_name: &str) {
    let paths: Box<dyn AppPathsProvider> = Box::new(DesktopPathsProvider);
    let prefs = UserPrefs::load(app_name, paths.as_ref());
    let backend = prefs.backend.unwrap_or_else(config::compile_time_backend);

    let platform = match WinitPlatform::try_new() {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Failed to create event loop: {e}");
            // Exit non-zero so launchers and scripts see the failed startup instead of a clean exit.
            std::process::exit(1);
        }
    };
    let AppConfig {
        mut window,
        font_paths,
        font_data,
    } = config;
    #[cfg(rsx_hot_reload)]
    apply_dev_window_overrides(&mut window);
    if let Some(custom) = app.window_config() {
        window = custom;
    }
    if let Err(e) = platform.run(
        window,
        AppHandler::<WinitWindow, D> {
            app: Box::new(app),
            tree: None,
            renderer: None,
            renderer_is_hardware: false,
            backend,
            prefs,
            pending_restart: false,
            pending_renderer: None,
            _flush_notify: None,
            scale_factor: 1.0,
            scale_scratch: renderer_core::ScaleScratch::new(),
            window_signals: None,
            app_name: app_name.to_owned(),
            last_frame: std::time::Instant::now(),
            dev: D::default(),
            paths,
            font_paths,
            font_data,
            _window: std::marker::PhantomData,
            render_tx: None,
            render_ret_rx: None,
            command_buf_pool: Vec::new(),
            render_join: None,
            hw_renderer: None,
            #[cfg(all(feature = "dev", not(target_os = "android")))]
            hot_reload_rx: None,
        },
    ) {
        tracing::error!("Event loop exited with error: {e}");
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "android"))]
pub fn run_app_with_name<A: App>(config: AppConfig, app: A, app_name: &str) {
    #[cfg(feature = "dev")]
    {
        // RSX_DEVTOOLS=0 disables the overlay even in a dev build.
        if std::env::var("RSX_DEVTOOLS").as_deref() == Ok("0") {
            run_desktop_with_plugin::<A, ()>(config, app, app_name);
        } else {
            run_desktop_with_plugin::<A, rsx_devtools::DevTools>(config, app, app_name);
        }
    }
    #[cfg(not(feature = "dev"))]
    run_desktop_with_plugin::<A, ()>(config, app, app_name);
}
