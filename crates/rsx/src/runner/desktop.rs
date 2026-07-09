use devtools_core::DevPlugin;
use platform_core::{PlatformError, SurfaceId};
use platform_desktop::{DesktopPathsProvider, WinitPlatform};
use services_core::AppPathsProvider;

use crate::app::App;
use crate::app_config::AppConfig;

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

fn run_desktop_with_plugin<A: App, D: DevPlugin>(config: AppConfig, app: A, app_name: &str) {
    let paths: Box<dyn AppPathsProvider> = Box::new(DesktopPathsProvider);
    let platform = match WinitPlatform::try_new() {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Failed to create event loop: {e}");
            // Exit non-zero so launchers and scripts see the failed startup instead of a clean exit.
            std::process::exit(1);
        }
    };
    let AppConfig {
        window,
        font_paths,
        font_data,
    } = config;
    #[cfg(rsx_hot_reload)]
    let window = {
        let mut window = window;
        apply_dev_window_overrides(&mut window);
        window
    };
    let config = AppConfig {
        window,
        font_paths,
        font_data,
    };
    if let Err(e) = super::run_with_platform::<_, A, D>(platform, config, paths, app, app_name) {
        tracing::error!("Event loop exited with error: {e}");
        std::process::exit(1);
    }
}

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

/// Open several native windows at once on the winit backend — the desktop counterpart to
/// [`crate::run_app_with_name`], and the multi-window entry a bar-per-monitor shell (or any multi-window app)
/// uses on a normal desktop. Each surface `(id, config)` gets a fresh app from `app_factory(id)` running on
/// **its own thread**, so it has a fully isolated reactive/theme/overlay/focus world. Returns once every window
/// has closed.
pub fn run_multi_app_with_name<A, AF>(
    surfaces: Vec<(SurfaceId, AppConfig)>,
    app_factory: AF,
    app_name: &str,
) -> Result<(), PlatformError>
where
    A: App,
    AF: Fn(SurfaceId) -> A + Send + Sync + 'static,
{
    let platform = WinitPlatform::try_new()?;
    super::run_multi_with_platform(
        platform,
        surfaces,
        |_id| Box::new(DesktopPathsProvider) as Box<dyn AppPathsProvider>,
        app_factory,
        app_name,
    )
}
