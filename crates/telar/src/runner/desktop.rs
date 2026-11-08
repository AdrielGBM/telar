//! Starting a desktop app, and opening a second window on the running loop.

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use platform_core::{PlatformError, SurfaceId};
use platform_desktop::{DesktopPathsProvider, WinitPlatform, WinitWindow, request_dynamic_surface};
use services_core::AppPathsProvider;
use ui_tree::DevOverlay;

use crate::app::App;
use crate::app_config::AppConfig;
use crate::surface::{SurfaceControl, SurfaceToken};

/// Installs the desktop's logging and services and opens its event loop, ending the process when there is no loop to open.
pub(super) fn start_desktop() -> WinitPlatform {
    super::logging::install_stderr();
    #[cfg(feature = "dialogs")]
    platform_desktop::DesktopFileDialogs::install();
    #[cfg(feature = "clipboard")]
    platform_desktop::DesktopClipboard::install();
    platform_desktop::DesktopUriOpener::install();
    match WinitPlatform::try_new() {
        Ok(platform) => platform,
        Err(e) => {
            tracing::error!("Failed to create event loop: {e}");
            // Exit non-zero so launchers and scripts see the failed startup instead of a clean exit.
            std::process::exit(1);
        }
    }
}

/// Ends the process with a failure when the event loop stopped on an error, so launchers and scripts see it.
pub(super) fn exit_on_error(result: Result<(), PlatformError>) {
    if let Err(e) = result {
        tracing::error!("Event loop exited with error: {e}");
        std::process::exit(1);
    }
}

fn run_desktop_with_overlay<A: App, D: DevOverlay>(config: AppConfig, app: A, app_name: &str) {
    let platform = start_desktop();
    let config = super::dev_window::with_dev_overrides(config);
    exit_on_error(super::generic::run_on_platform::<_, A, D>(
        platform,
        config,
        Arc::new(DesktopPathsProvider),
        app,
        app_name,
        super::host::SurfaceRenderer::builtin(),
        Some(super::location::LocationBinding::for_window(Box::new(
            platform_core::ArgumentLocation::from_env(),
        ))),
        None,
    ));
}

/// Starts an application on the desktop backend, with no overlay.
pub fn run_desktop_app_with_name<A: App>(config: AppConfig, app: A, app_name: &str) {
    run_desktop_app_with_devtools::<A, ()>(config, app, app_name)
}

/// The same, drawing `D` over the application.
///
/// `D` is any [`DevOverlay`]: it is handed the frame's draw commands and returns what to draw in their place, so an inspector of your own — a layout ruler, a state dump, an in-house profiler — composes over the tree exactly as the overlay of [`telar-devtools`](https://docs.rs/telar-devtools) does. `()` installs none.
///
/// `TELAR_DEVTOOLS=0` still turns it off, whichever overlay it is: the variable means *no overlay on this run*, not *not that one*.
pub fn run_desktop_app_with_devtools<A: App, D: DevOverlay>(
    config: AppConfig,
    app: A,
    app_name: &str,
) {
    if super::overlay_disabled() {
        run_desktop_with_overlay::<A, ()>(config, app, app_name);
    } else {
        run_desktop_with_overlay::<A, D>(config, app, app_name);
    }
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

/// Opens a **full `App`** in its own top-level window on the already-running single-thread multi-surface runner — the app is moved in (so it may be `!Send`, e.g. hold `Rc` state), keeps the one shared reactive runtime, and gets its own `Surface` world and `on_frame` driven. It hosts a real `App`, so a caller can move a live sub-app (a detached tab, with its state and background work) into a window. Returns a token; dropping it, or the window's own close, tears the window down. Only meaningful while [`run_app_windowed`] is running.
///
/// `config` is the window's, as [`run_app_windowed`] takes one: the app's own [`App::window_config`] wins over its window, whose title falls back to the running app's name, and its faces and family are what the window sets text in. The window is the running app's, so it keeps that app's directories and preferences.
///
/// # Panics
///
/// When no runner has installed the app's name, which [`run_app_windowed`] does before it opens its first window.
pub fn open_window<A: App>(config: AppConfig, app: A) -> SurfaceToken {
    let app_name = services_core::app_paths::name()
        .expect("open_window opens a window of a running app, so its runner has named it");
    let window = super::resolved_window(config.window.clone(), &app, &app_name);
    let handler = super::build_surface_handler::<WinitWindow, _>(
        crate::app_runtime::LocalApp(app),
        Arc::new(DesktopPathsProvider),
        &app_name,
        AppConfig {
            window: window.clone(),
            ..config
        },
    );
    let close = request_dynamic_surface(window, handler);
    SurfaceToken::new(Box::new(WinitSurfaceControl { close }))
}

/// Runs one app in a native window (like [`run_app_with_name`](crate::run_app_with_name)) but under the single-thread multi-surface runner — so the app can call [`open_window`] to move a live sub-app into a further top-level window that shares its one reactive runtime (e.g. a detached tab). The app may be `!Send`.
pub fn run_app_windowed<A: App>(config: AppConfig, app: A, app_name: &str) {
    let platform = start_desktop();
    let window = super::resolved_window(config.window.clone(), &app, app_name);
    let config = AppConfig { window, ..config };
    let app = RefCell::new(Some(app));
    exit_on_error(super::run_multi_with_platform(
        platform,
        vec![(SurfaceId(0), config)],
        |_id| Arc::new(DesktopPathsProvider) as Arc<dyn AppPathsProvider>,
        move |_id| {
            app.borrow_mut()
                .take()
                .expect("windowed app factory is called once")
        },
        app_name,
    ));
}
