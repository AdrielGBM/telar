//! The Android entry point: everything about booting an app that differs from every other platform.

use std::sync::Arc;

use crate::app::App;
use crate::app_config::AppConfig;
use crate::dev_plugin::DevPlugin;
use services_core::AppPathsProvider;

// App processes do not inherit the adb shell environment, so the engine's env-var debug flags are unreachable. Bridged from `debug.telar.<k>` system properties, which need no root. Must run before any `OnceLock` reads them or the render thread spawns.
#[cfg(feature = "runtime")]
fn bridge_debug_props_to_env() {
    for (prop, var) in [
        ("debug.telar.perf", "TELAR_PERF"),
        ("debug.telar.hw_damage", "TELAR_HW_DAMAGE"),
        ("debug.telar.scroll_blit", "TELAR_HW_SCROLL_BLIT"),
    ] {
        if std::env::var_os(var).is_some() {
            continue;
        }
        if let Some(v) = platform_android::read_sys_prop(prop) {
            if !v.is_empty() {
                unsafe { std::env::set_var(var, v) };
            }
        }
    }
}

#[cfg(feature = "runtime")]
/// Starts an application on the Android backend.
pub fn run_android_app_with_name<A: App>(
    config: AppConfig,
    app: A,
    app_name: &str,
    android_app: platform_android::AndroidApp,
) {
    bridge_debug_props_to_env();
    #[cfg(feature = "dev")]
    run_android_with_plugin::<A, crate::dev_tools::DevTools>(config, app, app_name, android_app);
    #[cfg(not(feature = "dev"))]
    run_android_with_plugin::<A, ()>(config, app, app_name, android_app);
}

/// Builds the Android platform and paths provider, then hands over to the one shared boot sequence.
///
/// The sequence itself — load prefs, resolve the backend, resolve the window, build the handler, run — used to be written out again here, `run_with_platform` being gated off this target for no reason: it is generic over `Platform`, `AndroidPlatform` implements it, and it imports nothing desktop-only.
#[cfg(feature = "runtime")]
fn run_android_with_plugin<A: App, D: DevPlugin>(
    config: AppConfig,
    app: A,
    app_name: &str,
    android_app: platform_android::AndroidApp,
) {
    use platform_android::{AndroidPathsProvider, AndroidPlatform};

    let paths: Arc<dyn AppPathsProvider> = Arc::new(AndroidPathsProvider::new(android_app.clone()));
    let platform = match AndroidPlatform::try_new(android_app) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Failed to create Android event loop: {e}");
            return;
        }
    };
    let config = super::dev_window::with_dev_overrides(config);
    if let Err(e) = super::run_with_platform::<_, A, D>(platform, config, paths, app, app_name) {
        tracing::error!("Android event loop exited with error: {e}");
    }
}
