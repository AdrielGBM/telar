//! The Android entry point: everything about booting an app that differs from every other platform.

use std::sync::Arc;

use crate::app::App;
use crate::app_config::AppConfig;
use services_core::AppPathsProvider;
use ui_tree::DevOverlay;

#[cfg(feature = "runtime")]
/// Starts an application on the Android backend, with no overlay.
pub fn run_android_app_with_name<A: App>(
    config: AppConfig,
    app: A,
    app_name: &str,
    android_app: platform_android::AndroidApp,
) {
    run_android_app_with_devtools::<A, ()>(config, app, app_name, android_app);
}

#[cfg(feature = "runtime")]
/// The same, drawing `D` over the application.
pub fn run_android_app_with_devtools<A: App, D: DevOverlay>(
    config: AppConfig,
    app: A,
    app_name: &str,
    android_app: platform_android::AndroidApp,
) {
    if super::overlay_disabled() {
        run_android_with_overlay::<A, ()>(config, app, app_name, android_app);
    } else {
        run_android_with_overlay::<A, D>(config, app, app_name, android_app);
    }
}

/// Builds the Android platform and paths provider, then hands over to the one shared boot sequence.
///
/// The sequence itself — read the stored backend, resolve the backend, resolve the window, build the handler, run — is `run_with_platform`'s, which is generic over `Platform` and imports nothing desktop-only.
#[cfg(feature = "runtime")]
fn run_android_with_overlay<A: App, D: DevOverlay>(
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
