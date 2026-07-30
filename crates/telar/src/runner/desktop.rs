use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use devtools_core::DevPlugin;
use platform_core::{EventHandler, PlatformError, SurfaceId, WindowConfig};
use platform_desktop::{DesktopPathsProvider, WinitPlatform, WinitWindow, request_dynamic_surface};
use renderer_core::Color;
use services_core::AppPathsProvider;
use ui_core::{Component, Surface, SurfacePlacement, SurfaceRoot, SurfaceScaffold, SurfaceSize};

use crate::app::App;
use crate::app_config::AppConfig;
use crate::surface::{SurfaceContent, SurfaceControl, SurfaceHost, SurfaceToken, set_surface_host};

#[cfg(telar_hot_reload)]
pub(super) fn apply_dev_window_overrides(config: &mut platform_core::WindowConfig) {
    if let Ok(v) = std::env::var("TELAR_DEV_WINDOW_TITLE") {
        config.title = v;
    }
    if let Ok(v) = std::env::var("TELAR_DEV_WINDOW_WIDTH") {
        if let Ok(n) = v.parse() {
            config.width = n;
        }
    }
    if let Ok(v) = std::env::var("TELAR_DEV_WINDOW_HEIGHT") {
        if let Ok(n) = v.parse() {
            config.height = n;
        }
    }
    if let Ok(v) = std::env::var("TELAR_DEV_WINDOW_DECORATIONS") {
        config.has_decorations = v == "1";
    }
    if let Ok(v) = std::env::var("TELAR_DEV_WINDOW_RESIZABLE") {
        config.is_resizable = v == "1";
    }
    if let Ok(v) = std::env::var("TELAR_DEV_WINDOW_TRANSPARENT") {
        config.is_transparent = v == "1";
    }
    if let Ok(v) = std::env::var("TELAR_DEV_WINDOW_FULLSCREEN") {
        config.fullscreen = match v.as_str() {
            "borderless" => platform_core::FullscreenMode::Borderless,
            "exclusive" => platform_core::FullscreenMode::Exclusive,
            _ => platform_core::FullscreenMode::Disabled,
        };
    }
    if let Ok(v) = std::env::var("TELAR_DEV_WINDOW_POSITION") {
        config.position = parse_dev_window_position(&v);
    }
}

// Parses the TELAR_DEV_WINDOW_POSITION value: "centered" (or empty/invalid) → Centered; "<x>,<y>" → absolute coordinates.
#[cfg(telar_hot_reload)]
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
    #[cfg(telar_hot_reload)]
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
        // TELAR_DEVTOOLS=0 disables the overlay even in a dev build.
        if std::env::var("TELAR_DEVTOOLS").as_deref() == Ok("0") {
            run_desktop_with_plugin::<A, ()>(config, app, app_name);
        } else {
            run_desktop_with_plugin::<A, telar_devtools::DevTools>(config, app, app_name);
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

// ---- Dynamic secondary surfaces (`open_surface`) on the winit backend --------------------------------

// Derives a winit `WindowConfig` from a backend-agnostic `SurfacePlacement`. winit has no compositor
// anchoring, so a placement becomes a borderless, transparent top-level window at its content size; a
// `needs_scaffold` placement (drawer/modal) relies on `SurfaceScaffold` to position the panel in-window.
fn window_config_for(placement: &SurfacePlacement) -> WindowConfig {
    let (width, height) = match placement.size {
        SurfaceSize::Fixed(w, h) => (w, h),
        SurfaceSize::Auto => (900, 660),
    };
    WindowConfig {
        title: "Surface".to_string(),
        width,
        height,
        has_decorations: false,
        is_transparent: true,
        ..Default::default()
    }
}

// Wraps an `open_surface` content closure as an `App` (mirrors hyprshell's `HostedSurfaceApp`). No
// `reset_layout_runtime`: the handler's own `Surface` supplies a fresh, isolated layout world.
struct HostedSurfaceApp {
    placement: SurfacePlacement,
    content: RefCell<Option<SurfaceContent>>,
}

impl App for HostedSurfaceApp {
    fn root(&self) -> Box<dyn Component> {
        let content = self
            .content
            .borrow_mut()
            .take()
            .expect("surface content built once")();
        if self.placement.needs_scaffold() {
            Box::new(
                SurfaceScaffold::new(&self.placement, content, None)
                    .expect("surface scaffold build failed")
                    .animate_in(),
            )
        } else {
            Box::new(
                SurfaceRoot::new(content)
                    .expect("surface root build failed")
                    .animate_in(),
            )
        }
    }

    fn clear_color(&self) -> Option<Color> {
        None
    }

    fn window_config(&self) -> Option<WindowConfig> {
        Some(window_config_for(&self.placement))
    }
}

// The winit `SurfaceHost`: `open_surface` builds a content-hosting handler (with its own `Surface`) and
// enqueues it as a new top-level window on the running runner — same thread, same reactive runtime as the
// parent, so the child shares the parent's signal graph.
struct WinitSurfaceHost;

impl SurfaceHost for WinitSurfaceHost {
    fn open(&self, placement: SurfacePlacement, content: SurfaceContent) -> SurfaceToken {
        let window_config = window_config_for(&placement);
        let app = HostedSurfaceApp {
            placement,
            content: RefCell::new(Some(content)),
        };
        let paths: Box<dyn AppPathsProvider> = Box::new(DesktopPathsProvider);
        let prefs = crate::prefs::UserPrefs::load("telar-surface", paths.as_ref());
        // Same backend convention as every other window: the resolved preference, else the compile-time
        // default (`Auto` = hardware with a software fallback). The tiling-WM resize race that once forced
        // software here is handled by deferring on_resume to the surface's first real `Resized` (see the
        // multi-surface runner), so secondary surfaces are first-class and render like the primary one.
        let backend = prefs
            .backend
            .unwrap_or_else(crate::config::compile_time_backend);
        // Fonts default to the system set; the layout-time text shaper is shared across surfaces (T-3.1), so
        // it already carries the parent's fonts.
        let mut handler = super::handler::build_app_handler::<WinitWindow, ()>(
            Box::new(app),
            paths,
            Vec::new(),
            Vec::new(),
            backend,
            prefs,
            "telar-surface".to_string(),
        );
        handler.surface = Some(Surface::new());
        let boxed: Box<dyn EventHandler<WinitWindow>> = Box::new(handler);
        let close = request_dynamic_surface(window_config, boxed);
        SurfaceToken::new(Box::new(WinitSurfaceControl { close }))
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

/// Opens a **full `App`** in its own top-level window on the already-running single-thread multi-surface
/// runner — the app is moved in (so it may be `!Send`, e.g. hold `Rc` state), keeps the one shared reactive
/// runtime, and gets its own `Surface` world and `on_frame` driven. Unlike `open_surface` (which hosts a
/// content closure), this hosts a real `App`, so a caller can move a live sub-app (a detached tab, with its
/// state and background work) into a window. Returns a token; dropping it, or the window's own close, tears
/// the window down. Only meaningful while `run_app_windowed`/the multi-surface runner is running.
pub fn open_window<A: App>(app: A) -> SurfaceToken {
    let window_config = app.window_config().unwrap_or_default();
    let paths: Box<dyn AppPathsProvider> = Box::new(DesktopPathsProvider);
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
        backend,
        prefs,
        "telar-window".to_string(),
    );
    handler.surface = Some(Surface::new());
    let boxed: Box<dyn EventHandler<WinitWindow>> = Box::new(handler);
    let close = request_dynamic_surface(window_config, boxed);
    SurfaceToken::new(Box::new(WinitSurfaceControl { close }))
}

/// Runs one app in a native window (like [`run_app_with_name`]) but under the single-thread multi-surface
/// runner with a real [`SurfaceHost`] installed — so the app can call `telar::open_surface` to spawn further
/// top-level windows that share its one reactive runtime (e.g. a detached tab). The app may be `!Send`.
pub fn run_app_windowed<A: App>(config: AppConfig, app: A, app_name: &str) {
    set_surface_host(Box::new(WinitSurfaceHost));
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
        |_id| Box::new(DesktopPathsProvider) as Box<dyn AppPathsProvider>,
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
