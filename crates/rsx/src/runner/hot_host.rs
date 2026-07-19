use platform_core::Window;
use renderer_core::RenderBackend;
use renderer_hardware::HardwareRenderer;

use super::FRAME_BUDGET;

#[cfg(all(feature = "dev", not(target_os = "android")))]
use platform_core::Platform;
#[cfg(all(feature = "dev", not(target_os = "android")))]
use platform_desktop::{WinitPlatform, WinitWindow};

#[cfg(all(feature = "dev", not(target_os = "android")))]
use crate::app::App;
#[cfg(all(feature = "dev", not(target_os = "android")))]
use crate::config;
#[cfg(all(feature = "dev", not(target_os = "android")))]
use crate::prefs::UserPrefs;
#[cfg(all(feature = "dev", not(target_os = "android")))]
use platform_desktop::DesktopPathsProvider;

#[cfg(all(feature = "dev", not(target_os = "android")))]
use super::handler::build_app_handler;

pub(super) struct HardwareFrameMsg {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) scale_factor: f32,
    pub(super) generation: u64,
    pub(super) commands: Vec<renderer_core::DrawCommand>,
    pub(super) clear: Option<renderer_core::Color>,
    pub(super) timestamp: std::time::Instant,
}

pub(super) fn spawn_hardware_render_thread<W>(
    renderer: HardwareRenderer<W>,
) -> (
    std::sync::mpsc::SyncSender<HardwareFrameMsg>,
    std::sync::mpsc::Receiver<Vec<renderer_core::DrawCommand>>,
    std::thread::JoinHandle<HardwareRenderer<W>>,
)
where
    W: Window + Clone + Send + Sync + 'static,
{
    let (tx, rx) = std::sync::mpsc::sync_channel::<HardwareFrameMsg>(1);
    // F2: hand the consumed command buffer back to the UI thread so it refills the same allocation
    // next frame instead of freeing it here and allocating a fresh Vec every frame.
    let (ret_tx, ret_rx) = std::sync::mpsc::channel::<Vec<renderer_core::DrawCommand>>();
    let join = std::thread::Builder::new()
        .name("rsx-render".to_string())
        .spawn(move || {
            let mut renderer = renderer;
            let mut current_width = 0u32;
            let mut current_height = 0u32;
            // ADPF lives on THIS thread: create the hint session with the render thread's own TID (None self-computes SYS_gettid here) so reportActualWorkDuration drives the scheduler for the thread that actually submits GPU work. The session is not Send, so it is created, used, and dropped here and never crosses a thread boundary. (The SW/fallback path keeps its own session on the UI thread.)
            #[cfg(target_os = "android")]
            let hint_session = platform_android::AdpfSession::new(16_666_667, None);
            while let Ok(msg) = rx.recv() {
                // Drop stale frames to stay responsive, but never skip one that resizes the surface: the wgpu surface is reconfigured inside begin_frame, so a dropped resize frame leaves it at the old size and the window shows clipped content or empty margins until the next accepted frame.
                let size_changed = msg.width != current_width || msg.height != current_height;
                if !size_changed && msg.timestamp.elapsed() > FRAME_BUDGET {
                    let _ = ret_tx.send(msg.commands);
                    continue;
                }
                #[cfg(target_os = "android")]
                let frame_start = std::time::Instant::now();
                // begin_frame reconfigures the swapchain and recreates size-dependent textures; a wgpu fatal
                // error there (e.g. a lost device after a compositor resize storm) is a panic, not an `Err`, so
                // catch it as render_frame does below and drop the frame instead of unwinding into an abort.
                let began = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    renderer.begin_frame(msg.width, msg.height, msg.scale_factor, msg.generation)
                }));
                if !matches!(began, Ok(Ok(()))) {
                    let _ = ret_tx.send(msg.commands);
                    continue;
                }
                current_width = msg.width;
                current_height = msg.height;
                // A wgpu validation error (e.g. a transient scissor/surface-size mismatch while a compositor
                // resizes a just-opened window) is fatal by default and would abort the whole process from
                // this render thread. Catch it and drop the frame so the app survives and recovers on the
                // next, correctly-sized frame. (Under panic=unwind only; a panic=abort release build aborts.)
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    renderer.render_frame(&msg.commands, msg.clear)
                }));
                #[cfg(target_os = "android")]
                if let Some(session) = &hint_session {
                    let duration_ns = frame_start.elapsed().as_nanos() as i64;
                    session.report(duration_ns);
                }
                // Recycle the buffer for the UI thread to refill; a send failure (UI gone) just drops it.
                let _ = ret_tx.send(msg.commands);
            }
            // hint_session drops here (closeSession) on this render thread before it exits.
            // Return the renderer so on_suspend can reclaim it and keep warm caches across resume.
            renderer
        })
        .expect("failed to spawn render thread");
    (tx, ret_rx, join)
}

#[cfg(all(feature = "dev", not(target_os = "android")))]
pub fn run_hot_reload_host(
    lib_path: &str,
    hot_port: &str,
    config: crate::app_config::AppConfig,
    app_name: &str,
) {
    let Ok(port) = hot_port.parse::<u16>() else {
        tracing::error!("invalid RSX_HOT_PORT value: {hot_port}");
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
    let paths: Box<dyn services_core::AppPathsProvider> = Box::new(DesktopPathsProvider);
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
    #[cfg(rsx_hot_reload)]
    super::desktop::apply_dev_window_overrides(&mut window);
    if let Some(custom) = initial_app.window_config() {
        window = custom;
    }
    // Share the one field literal with `run_with_platform` (via build_app_handler); only the hot-reload
    // receiver differs from a normal single-window handler.
    let mut handler = build_app_handler::<WinitWindow, rsx_devtools::DevTools>(
        Box::new(initial_app),
        paths,
        font_paths,
        font_data,
        backend,
        prefs,
        app_name.to_owned(),
    );
    handler.hot_reload_rx = Some(hot_rx);
    if let Err(e) = platform.run(window, handler) {
        tracing::error!("Event loop error: {e}");
        std::process::exit(1);
    }
}
