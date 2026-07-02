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
use super::handler::AppHandler;

#[cfg(target_os = "android")]
pub(super) mod adpf {
    use std::ffi::c_long;

    #[link(name = "android")]
    unsafe extern "C" {
        pub fn APerformanceHint_getManager() -> *mut std::ffi::c_void;
        pub fn APerformanceHint_createSession(
            manager: *mut std::ffi::c_void,
            thread_ids: *const i32,
            size: usize,
            initial_target_work_duration_ns: c_long,
        ) -> *mut std::ffi::c_void;
        pub fn APerformanceHint_reportActualWorkDuration(
            session: *mut std::ffi::c_void,
            actual_duration_ns: c_long,
        );
        pub fn APerformanceHint_closeSession(session: *mut std::ffi::c_void);
    }
}

pub(super) fn android_sans_serif_candidates() -> Vec<String> {
    vec![
        "Roboto".to_string(),
        "Droid Sans".to_string(),
        "MiSans Latin".to_string(),
        "Noto Sans".to_string(),
    ]
}

#[cfg(all(feature = "runtime", target_os = "android"))]
pub fn run_android_app_with_name<A: App>(
    config: AppConfig,
    app: A,
    app_name: &str,
    android_app: platform_android::AndroidApp,
) {
    #[cfg(feature = "dev")]
    run_android_with_plugin::<A, rsx_devtools::DevTools>(config, app, app_name, android_app);
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
    #[cfg(rsx_hot_reload)]
    super::desktop::apply_dev_window_overrides(&mut window);
    if let Some(custom) = app.window_config() {
        window = custom;
    }
    if let Err(e) = platform.run(
        window,
        AppHandler::<AndroidWindow, D> {
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
            render_join: None,
            hw_renderer: None,
            #[cfg(all(feature = "dev", not(target_os = "android")))]
            hot_reload_rx: None,
            hint_session: None,
            frame_start: std::time::Instant::now(),
        },
    ) {
        tracing::error!("Android event loop exited with error: {e}");
    }
}
