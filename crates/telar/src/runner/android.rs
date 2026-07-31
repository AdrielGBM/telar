#[cfg(target_os = "android")]
use devtools_core::DevPlugin;
#[cfg(target_os = "android")]
use platform_core::Platform;
#[cfg(target_os = "android")]
use services_core::AppPathsProvider;

#[cfg(target_os = "android")]
use crate::app::App;
#[cfg(target_os = "android")]
use crate::app_config::AppConfig;
#[cfg(target_os = "android")]
use crate::config;
#[cfg(target_os = "android")]
use crate::prefs::UserPrefs;

#[cfg(target_os = "android")]
use super::handler::build_app_handler;

// App processes do not inherit the adb shell environment, so debug flags that are env vars on
// desktop (TELAR_PERF, TELAR_HW_DAMAGE, …) are unreachable on Android. Bridge them from `debug.telar.<k>`
// system properties (settable without root via `adb shell setprop debug.telar.perf 1`) into the
// env vars the engine reads. Must run before any OnceLock reads them or the render thread spawns.
#[cfg(all(feature = "runtime", target_os = "android"))]
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

#[cfg(all(feature = "runtime", target_os = "android"))]
pub fn run_android_app_with_name<A: App>(
    config: AppConfig,
    app: A,
    app_name: &str,
    android_app: platform_android::AndroidApp,
) {
    bridge_debug_props_to_env();
    #[cfg(feature = "dev")]
    run_android_with_plugin::<A, telar_devtools::DevTools>(config, app, app_name, android_app);
    #[cfg(not(feature = "dev"))]
    run_android_with_plugin::<A, ()>(config, app, app_name, android_app);
}

#[cfg(all(feature = "runtime", target_os = "android"))]
fn run_android_with_plugin<A: App, D: DevPlugin>(
    config: AppConfig,
    app: A,
    app_name: &str,
    android_app: platform_android::AndroidApp,
) {
    use platform_android::{AndroidPathsProvider, AndroidPlatform, AndroidWindow};

    let paths: Box<dyn AppPathsProvider> = Box::new(AndroidPathsProvider::new(android_app.clone()));
    let prefs = UserPrefs::load(app_name, paths.as_ref());
    let backend = prefs.backend.unwrap_or_else(config::compile_time_backend);

    let platform = match AndroidPlatform::try_new(android_app) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Failed to create Android event loop: {e}");
            return;
        }
    };
    let AppConfig {
        mut window,
        font_paths,
        font_data,
    } = config;
    #[cfg(telar_hot_reload)]
    super::desktop::apply_dev_window_overrides(&mut window);
    if let Some(custom) = app.window_config() {
        window = custom;
    }
    if let Err(e) = platform.run(
        window,
        build_app_handler::<AndroidWindow, D>(
            Box::new(app),
            paths,
            font_paths,
            font_data,
            backend,
            prefs,
            app_name.to_owned(),
        ),
    ) {
        tracing::error!("Android event loop exited with error: {e}");
    }
}
