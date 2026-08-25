use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::dev_plugin::DevPlugin;
use platform_core::{EventHandler, SurfaceId};
use platform_desktop::{DesktopPathsProvider, WinitPlatform, WinitWindow, request_dynamic_surface};
use services_core::AppPathsProvider;
use ui_core::Surface;

use crate::app::App;
use crate::app_config::AppConfig;
use crate::surface::{SurfaceControl, SurfaceToken};

fn run_desktop_with_plugin<A: App, D: DevPlugin>(config: AppConfig, app: A, app_name: &str) {
    let paths: Arc<dyn AppPathsProvider> = Arc::new(DesktopPathsProvider);
    platform_desktop::DesktopFileDialogs::install();
    platform_desktop::DesktopClipboard::install();
    let platform = match WinitPlatform::try_new() {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Failed to create event loop: {e}");
            // Exit non-zero so launchers and scripts see the failed startup instead of a clean exit.
            std::process::exit(1);
        }
    };
    let config = super::dev_window::with_dev_overrides(config);
    if let Err(e) = super::run_with_platform::<_, A, D>(platform, config, paths, app, app_name) {
        tracing::error!("Event loop exited with error: {e}");
        std::process::exit(1);
    }
}

pub fn run_app_with_name<A: App>(config: AppConfig, app: A, app_name: &str) {
    #[cfg(feature = "dev")]
    {
        // TELAR_DEVTOOLS=0 disables the overlay even in a dev build.
        if std::env::var("TELAR_DEVTOOLS").as_deref() == Ok("0") {
            run_desktop_with_plugin::<A, ()>(config, app, app_name);
        } else {
            run_desktop_with_plugin::<A, crate::dev_tools::DevTools>(config, app, app_name);
        }
    }
    #[cfg(not(feature = "dev"))]
    run_desktop_with_plugin::<A, ()>(config, app, app_name);
}

struct WinitSurfaceControl {
    close: Arc<AtomicBool>,
}

impl SurfaceControl for WinitSurfaceControl {
    fn close(&self) {
        self.close.store(true, Ordering::Relaxed);
    }
    fn is_closing(&self) -> bool {
        self.close.load(Ordering::Relaxed)
    }
}

/// Opens a **full `App`** in its own top-level window on the already-running single-thread multi-surface
/// runner — the app is moved in (so it may be `!Send`, e.g. hold `Rc` state), keeps the one shared reactive
/// runtime, and gets its own `Surface` world and `on_frame` driven. Unlike `open_surface` (which hosts a
/// content closure), this hosts a real `App`, so a caller can move a live sub-app (a detached tab, with its
/// state and background work) into a window. Returns a token; dropping it, or the window's own close, tears
/// the window down. Only meaningful while `run_app_windowed`/the multi-surface runner is running.
pub fn open_window<A: App>(app: A) -> SurfaceToken {
    let window_config = app.window_config().unwrap_or_default();
    let paths: Arc<dyn AppPathsProvider> = Arc::new(DesktopPathsProvider);
    platform_desktop::DesktopFileDialogs::install();
    platform_desktop::DesktopClipboard::install();
    let prefs = crate::prefs::UserPrefs::load("telar-window", paths.as_ref());
    // Same backend convention as the primary window (resolved preference, else the compile-time default —
    // `Auto` = hardware with a software fallback): a secondary window is a first-class window.
    let backend = prefs
        .backend
        .unwrap_or_else(crate::config::compile_time_backend);
    let mut handler = super::handler::build_app_handler::<WinitWindow, ()>(
        Box::new(app),
        paths,
        Vec::new(),
        Vec::new(),
        None,
        backend,
        prefs,
        "telar-window".to_string(),
        super::host::SurfaceRenderer::builtin(),
    );
    handler.surface = Some(Surface::new());
    let boxed: Box<dyn EventHandler<WinitWindow>> = Box::new(handler);
    let close = request_dynamic_surface(window_config, boxed);
    SurfaceToken::new(Box::new(WinitSurfaceControl { close }))
}

/// Runs one app in a native window (like [`run_app_with_name`]) but under the single-thread multi-surface
/// runner — so the app can call [`open_window`] to move a live sub-app into a further top-level window that
/// shares its one reactive runtime (e.g. a detached tab). The app may be `!Send`.
pub fn run_app_windowed<A: App>(config: AppConfig, app: A, app_name: &str) {
    let platform = match WinitPlatform::try_new() {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Failed to create event loop: {e}");
            std::process::exit(1);
        }
    };
    // The app's own window_config (title/size/decorations) wins over the AppConfig default.
    let window = app.window_config().unwrap_or_else(|| config.window.clone());
    let config = AppConfig { window, ..config };
    let app = RefCell::new(Some(app));
    let result = super::run_multi_with_platform(
        platform,
        vec![(SurfaceId(0), config)],
        |_id| Arc::new(DesktopPathsProvider) as Arc<dyn AppPathsProvider>,
        move |_id| {
            app.borrow_mut()
                .take()
                .expect("windowed app factory is called once")
        },
        app_name,
    );
    if let Err(e) = result {
        tracing::error!("Event loop exited with error: {e}");
        std::process::exit(1);
    }
}
