#[cfg(target_os = "android")]
mod adpf {
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

use platform_core::{Event, EventHandler, Platform, Window};
use reactive_core::{FlushNotifyHandle, begin_batch, end_batch, set_flush_notify};
use renderer_core::{RenderBackend, RendererError};
use renderer_software::{SoftwareRenderer, SoftwareRendererConfig};
use services_core::AppPathsProvider;
use ui_core::{ComponentList, EventResult};

struct HardwareFrameMsg {
    width: u32,
    height: u32,
    scale_factor: f32,
    generation: u64,
    commands: Vec<renderer_core::DrawCommand>,
    clear: Option<renderer_core::Color>,
    timestamp: std::time::Instant,
}

use devtools_core::{DevAction, DevPlugin};

use crate::app::App;
use crate::app_config::AppConfig;
use crate::config::{self, RendererBackend};
use crate::prefs::UserPrefs;
use crate::window_signals::WindowSignals;

#[cfg(all(feature = "runtime", not(target_os = "android")))]
use crate::paths::DesktopPathsProvider;
#[cfg(not(target_os = "android"))]
use platform_winit::{WinitPlatform, WinitWindow};
use renderer_hardware::{HardwareRenderer, HardwareRendererConfig};

struct AppHandler<W, D: DevPlugin>
where
    W: Window + Clone + Send + Sync + 'static,
{
    app: Box<dyn App>,
    tree: Option<ComponentList>,
    renderer: Option<Box<dyn RenderBackend>>,
    renderer_is_hardware: bool,
    backend: RendererBackend,
    prefs: UserPrefs,
    paths: Box<dyn AppPathsProvider>,
    pending_restart: bool,
    pending_renderer: Option<std::thread::JoinHandle<Result<HardwareRenderer<W>, RendererError>>>,
    _flush_notify: Option<FlushNotifyHandle>,
    scale_factor: f32,
    // Reused across frames so the SW/HiDPI command scaling allocates neither a fresh Vec nor redundant per-command style Arcs.
    scale_scratch: renderer_core::ScaleScratch,
    window_signals: Option<WindowSignals>,
    app_name: String,
    last_frame: std::time::Instant,
    dev: D,
    font_paths: Vec<std::path::PathBuf>,
    font_data: Vec<Vec<u8>>,
    _window: std::marker::PhantomData<W>,
    render_tx: Option<std::sync::mpsc::SyncSender<HardwareFrameMsg>>,
    render_join: Option<std::thread::JoinHandle<HardwareRenderer<W>>>,
    hw_renderer: Option<HardwareRenderer<W>>,
    #[cfg(all(feature = "dev", not(target_os = "android")))]
    hot_reload_rx: Option<std::sync::mpsc::Receiver<crate::hot::HotEvent>>,
    #[cfg(target_os = "android")]
    hint_session: Option<*mut std::ffi::c_void>,
    #[cfg(target_os = "android")]
    frame_start: std::time::Instant,
}

const FRAME_BUDGET: std::time::Duration = std::time::Duration::from_nanos(1_000_000_000 / 60);

impl<W, D> EventHandler<W> for AppHandler<W, D>
where
    W: Window + Clone + Send + Sync + 'static,
    D: DevPlugin,
{
    fn on_resume(&mut self, window: &W) -> bool {
        let android = cfg!(target_os = "android");
        // Point the layout-time text measurer at the same fonts as the renderer, on this (the layout) thread, before any layout runs. Otherwise it falls back to system defaults and aborts on Android ("no default font found").
        renderer_text::set_measure_font_config(build_font_config(
            self.font_paths.clone(),
            self.font_data.clone(),
            android,
        ));
        let cache_path = hardware_cache_path(&self.app_name, self.paths.as_ref());
        match self.backend {
            RendererBackend::Software => {
                let budget = build_software_renderer_config(
                    self.font_paths.clone(),
                    self.font_data.clone(),
                    android,
                );
                match SoftwareRenderer::new(window.clone(), window.clone(), budget) {
                    Ok(r) => {
                        self.renderer = Some(Box::new(r));
                    }
                    Err(e) => {
                        tracing::error!("SW renderer failed: {e}");
                        return false;
                    }
                }
            }
            RendererBackend::Hardware | RendererBackend::Auto => {
                let font_config = build_hardware_font_config(
                    self.font_paths.clone(),
                    self.font_data.clone(),
                    android,
                );
                // Reuse the renderer saved on suspend (keeps device/pipelines/caches warm); only the surface is rebound. Otherwise build a fresh one.
                let hw_result = if let Some(mut existing) = self.hw_renderer.take() {
                    existing
                        .rebind_surface(std::sync::Arc::new(window.clone()))
                        .map(|()| existing)
                } else {
                    HardwareRenderer::new(
                        window.clone(),
                        cache_path.as_deref(),
                        android,
                        font_config,
                        HardwareRendererConfig::default(),
                    )
                };
                match hw_result {
                    Ok(hw) => {
                        let (tx, join) = spawn_hardware_render_thread(hw);
                        self.render_tx = Some(tx);
                        self.render_join = Some(join);
                        self.renderer_is_hardware = true;
                    }
                    Err(e) if matches!(self.backend, RendererBackend::Auto) => {
                        tracing::warn!("HW renderer unavailable ({e}), falling back to SW");
                        let budget = build_software_renderer_config(
                            self.font_paths.clone(),
                            self.font_data.clone(),
                            android,
                        );
                        match SoftwareRenderer::new(window.clone(), window.clone(), budget) {
                            Ok(r) => {
                                self.renderer = Some(Box::new(r));
                            }
                            Err(e2) => {
                                tracing::error!("SW fallback also failed: {e2}");
                                return false;
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("HW renderer failed: {e}");
                        return false;
                    }
                }
            }
        }
        let sf = window.scale_factor() as f32;
        self.scale_factor = sf;
        self.window_signals = Some(WindowSignals::new(
            window.width() as f32 / sf,
            window.height() as f32 / sf,
        ));
        self.tree = Some(ComponentList::new(self.app.root()));
        #[cfg(all(feature = "dev", not(target_os = "android")))]
        if let Some(rx) = self.hot_reload_rx.take() {
            let (relay_tx, relay_rx) = std::sync::mpsc::channel::<crate::hot::HotEvent>();
            let window_clone = window.clone();
            std::thread::Builder::new()
                .name("rsx-hot-relay".to_string())
                .spawn(move || {
                    while let Ok(event) = rx.recv() {
                        if relay_tx.send(event).is_err() {
                            break;
                        }
                        window_clone.request_redraw();
                    }
                })
                .ok();
            self.hot_reload_rx = Some(relay_rx);
        }
        // Synthesize an initial WindowResized so apps that initialize layout from that event start with the correct logical dimensions instead of their hardcoded defaults.
        let initial_resize = platform_core::Event::WindowResized {
            width: (window.width() as f32 / sf) as u32,
            height: (window.height() as f32 / sf) as u32,
        };
        if let Some(ref mut tree) = self.tree {
            tree.on_event(&initial_resize);
        }

        let w = window.clone();
        self._flush_notify = Some(set_flush_notify(move || w.request_redraw()));
        // Only the SW/fallback path reports ADPF from this (the UI/layout) thread, so only it needs a session keyed to this TID. The HW path renders on a dedicated thread and creates its own session there (correct TID); creating one here too would register the wrong thread.
        #[cfg(target_os = "android")]
        if !self.renderer_is_hardware {
            let session = unsafe {
                let manager = adpf::APerformanceHint_getManager();
                if manager.is_null() {
                    None
                } else {
                    let tid = libc::syscall(libc::SYS_gettid) as i32;
                    let s = adpf::APerformanceHint_createSession(manager, &tid, 1, 16_666_667);
                    if s.is_null() { None } else { Some(s) }
                }
            };
            self.hint_session = session;
        }
        window.request_redraw();
        true
    }

    fn on_event(&mut self, event: Event, window: &W) {
        if let Event::ScaleFactorChanged { scale_factor } = &event {
            self.scale_factor = *scale_factor as f32;
        }
        if let Event::WindowResized { width, height } = &event {
            if let Some(ref signals) = self.window_signals {
                signals.update(*width as f32, *height as f32);
            }
        }
        if let Event::KeyPressed { key, modifiers } = &event {
            match self.dev.on_key(key, *modifiers) {
                DevAction::Redraw => {
                    window.request_redraw();
                }
                DevAction::ToggleBackend => {
                    let next = match self.prefs.backend.unwrap_or(RendererBackend::Auto) {
                        RendererBackend::Hardware => RendererBackend::Software,
                        _ => RendererBackend::Hardware,
                    };
                    self.prefs.backend = Some(next);
                    if let Err(e) = self.prefs.save(&self.app_name, self.paths.as_ref()) {
                        tracing::warn!("Could not save preferences: {e}");
                    }
                    match next {
                        RendererBackend::Software => {
                            self.pending_restart = true;
                        }
                        _ => {
                            let window_clone = window.clone();
                            let cache_path =
                                hardware_cache_path(&self.app_name, self.paths.as_ref());
                            let font_paths = self.font_paths.clone();
                            let font_data = self.font_data.clone();
                            let android = cfg!(target_os = "android");
                            let handle = std::thread::spawn(move || {
                                let font_config =
                                    build_hardware_font_config(font_paths, font_data, android);
                                HardwareRenderer::new(
                                    window_clone,
                                    cache_path.as_deref(),
                                    android,
                                    font_config,
                                    HardwareRendererConfig::default(),
                                )
                            });
                            self.pending_renderer = Some(handle);
                        }
                    }
                }
                DevAction::None => {}
            }
        }
        if let Event::PointerPressed { x, y, .. } = &event {
            if self.dev.on_pointer_pressed(*x as f32, *y as f32) {
                window.request_redraw();
                return;
            }
        }
        if let Some(tree) = &mut self.tree {
            if tree.on_event(&event) == EventResult::Handled {
                #[cfg(feature = "dev")]
                tree.bump_force_ticks();
                // Flush reactive effects immediately so on_redraw() in the same cycle finds tree_dirty=true rather than deferring to the next cycle.
                end_batch();
                begin_batch();
                window.request_redraw();
            }
        }
    }

    fn on_redraw(&mut self, window: &W) {
        #[cfg(all(feature = "dev", not(target_os = "android")))]
        if let Some(rx) = &self.hot_reload_rx {
            if let Ok(event) = rx.try_recv() {
                match event {
                    crate::hot::HotEvent::Reload(new_path) => {
                        match crate::hot::load_hot_app(&new_path) {
                            Ok(new_app) => {
                                // Drop the old tree first so effect closures (which contain code from the old dylib) are destroyed while the old lib is still mapped. Only then replace self.app, which dlcloses the old dylib.
                                self.tree = None;
                                self.app = Box::new(new_app);
                                self.tree = Some(ComponentList::new(self.app.root()));
                                // A successful reload clears any banner from the previous failed build.
                                self.dev.set_build_error(None);
                                // Synthesize WindowResized so the new tree's layout starts with the correct logical dimensions instead of its 0×0 defaults.
                                let resize = platform_core::Event::WindowResized {
                                    width: (window.width() as f32 / self.scale_factor) as u32,
                                    height: (window.height() as f32 / self.scale_factor) as u32,
                                };
                                if let Some(ref mut tree) = self.tree {
                                    tree.on_event(&resize);
                                    tree.bump_force_ticks();
                                }
                                tracing::info!("hot reloaded: {}", new_path.display());
                                window.request_redraw();
                                return;
                            }
                            Err(e) => tracing::error!("hot reload failed: {e}"),
                        }
                    }
                    crate::hot::HotEvent::BuildError(msg) => {
                        self.dev.set_build_error(Some(msg));
                        window.request_redraw();
                    }
                }
            }
        }
        let mut redraw_requested = false;
        {
            let mut ctx = crate::app_context::AppCtx {
                app_name: &self.app_name,
                prefs: &mut self.prefs,
                paths: self.paths.as_ref(),
                pending_restart: &mut self.pending_restart,
                redraw_requested: &mut redraw_requested,
                window_signals: self.window_signals.as_ref(),
            };
            self.app.on_frame(&mut ctx);
        }
        if redraw_requested {
            window.request_redraw();
        }

        if let Some(handle) = self.pending_renderer.take() {
            if handle.is_finished() {
                match handle.join().unwrap_or_else(|_| {
                    Err(RendererError::Backend(
                        "renderer thread panicked".to_string(),
                    ))
                }) {
                    Ok(new_renderer) => {
                        drop(self.renderer.take());
                        // Drop the old render sender to signal the old render thread to exit, then wait for it.
                        drop(self.render_tx.take());
                        if let Some(j) = self.render_join.take() {
                            let _ = j.join();
                        }
                        #[cfg(target_os = "linux")]
                        unsafe {
                            libc::malloc_trim(0);
                        }
                        let (tx, join) = spawn_hardware_render_thread(new_renderer);
                        self.render_tx = Some(tx);
                        self.render_join = Some(join);
                        self.renderer_is_hardware = true;
                    }
                    Err(e) => tracing::error!("Background HW renderer creation failed: {e}"),
                }
                window.request_redraw();
            } else {
                self.pending_renderer = Some(handle);
            }
        }

        if self.pending_restart {
            self.pending_restart = false;
            self.backend = self
                .prefs
                .backend
                .unwrap_or_else(config::compile_time_backend);
            let cache_path = hardware_cache_path(&self.app_name, self.paths.as_ref());
            let android = cfg!(target_os = "android");
            // Drop old renderer and render thread before creating new one to avoid peak memory overlap.
            drop(self.renderer.take());
            drop(self.render_tx.take());
            if let Some(j) = self.render_join.take() {
                let _ = j.join();
            }
            #[cfg(target_os = "linux")]
            unsafe {
                libc::malloc_trim(0);
            }
            match self.backend {
                RendererBackend::Software => {
                    let budget = build_software_renderer_config(
                        self.font_paths.clone(),
                        self.font_data.clone(),
                        android,
                    );
                    match SoftwareRenderer::new(window.clone(), window.clone(), budget) {
                        Ok(r) => {
                            self.renderer = Some(Box::new(r));
                            self.renderer_is_hardware = false;
                        }
                        Err(e) => tracing::error!("Failed to switch to SW renderer: {e}"),
                    }
                }
                RendererBackend::Hardware | RendererBackend::Auto => {
                    let font_config = build_hardware_font_config(
                        self.font_paths.clone(),
                        self.font_data.clone(),
                        android,
                    );
                    match HardwareRenderer::new(
                        window.clone(),
                        cache_path.as_deref(),
                        android,
                        font_config,
                        HardwareRendererConfig::default(),
                    ) {
                        Ok(hw) => {
                            let (tx, join) = spawn_hardware_render_thread(hw);
                            self.render_tx = Some(tx);
                            self.render_join = Some(join);
                            self.renderer_is_hardware = true;
                        }
                        Err(e) => tracing::error!("Failed to switch to HW renderer: {e}"),
                    }
                }
            }
        }

        let tree_dirty = self.tree.as_ref().map(|t| t.is_dirty()).unwrap_or(false);
        // HW render thread always receives frames (idle-blit avoids GPU wake-up); SW skips when nothing changed unless dev plugin requests keepalive.
        let needs_keepalive = self.render_tx.is_some()
            || self.renderer_is_hardware
            || self.dev.keepalive_interval().is_some();
        if !tree_dirty && !needs_keepalive {
            return;
        }

        // 60 FPS cap: defer this redraw until the frame budget expires; about_to_wait() schedules the WaitUntil wakeup.
        if tree_dirty && self.last_frame.elapsed() < FRAME_BUDGET {
            return;
        }
        // Only update last_frame for content frames; keepalive blits must not reset the budget clock (would delay next content render by up to 16ms).
        if tree_dirty {
            self.last_frame = std::time::Instant::now();
        }

        let (w, h) = (window.width(), window.height());
        tracing::debug!(
            "on_redraw: window {}x{} scale={} tree_dirty={}",
            w,
            h,
            self.scale_factor,
            tree_dirty
        );

        // HW render thread path: build commands and send to dedicated render thread; main thread returns immediately.
        if let Some(tx) = &self.render_tx {
            end_batch();
            begin_batch();
            let clear = self.app.clear_color();
            let commands_ref = self.tree.as_ref().map(|t| t.commands());
            let base_slice: &[renderer_core::DrawCommand] =
                commands_ref.as_deref().map(|r| r.as_slice()).unwrap_or(&[]);
            if let Some(tree) = &self.tree {
                self.dev.on_tree(tree);
            }
            let logical_w = w as f32 / self.scale_factor;
            let logical_h = h as f32 / self.scale_factor;
            let frame_commands = self
                .dev
                .on_frame(base_slice, logical_w, logical_h, tree_dirty);
            let commands = frame_commands.to_vec();
            let msg = HardwareFrameMsg {
                width: w,
                height: h,
                scale_factor: self.scale_factor,
                generation: self.tree.as_ref().map(|t| t.generation()).unwrap_or(0),
                commands,
                clear,
                timestamp: std::time::Instant::now(),
            };
            // Drop frame if render thread is busy; keeps the main thread responsive.
            let _ = tx.try_send(msg);
            return;
        }

        // SW path: renderer must be present to proceed.
        let Some(renderer) = &mut self.renderer else {
            return;
        };

        #[cfg(target_os = "android")]
        {
            self.frame_start = std::time::Instant::now();
        }
        if let Err(e) = renderer.begin_frame(
            w,
            h,
            self.scale_factor,
            self.tree.as_ref().map(|t| t.generation()).unwrap_or(0),
        ) {
            tracing::error!("begin_frame failed: {e}");
            return;
        }
        // Flush reactive effects so clear_color and draw commands are from the same reactive pass. Without this, a RedrawRequested that fires before about_to_wait (e.g. HW keepalive) reads clear_color from the new signal value while commands still reflect the previous view() call.
        end_batch();
        begin_batch();
        let clear = self.app.clear_color();
        let commands_ref = self.tree.as_ref().map(|t| t.commands());
        let base_slice: &[renderer_core::DrawCommand] =
            commands_ref.as_deref().map(|r| r.as_slice()).unwrap_or(&[]);
        if let Some(tree) = &self.tree {
            self.dev.on_tree(tree);
        }
        let logical_w = w as f32 / self.scale_factor;
        let logical_h = h as f32 / self.scale_factor;
        let frame_commands = self
            .dev
            .on_frame(base_slice, logical_w, logical_h, tree_dirty);
        let frame_commands: &[renderer_core::DrawCommand] = &frame_commands;
        // Scale into a reusable buffer (no per-frame Vec) reusing the scaled Arc for styles shared across commands (no redundant per-command Arc::new). HW scales in the shader, so this only runs on the SW/HiDPI fallback path.
        let frame_commands = if self.scale_factor != 1.0 {
            self.scale_scratch
                .scale_into(frame_commands, self.scale_factor)
        } else {
            frame_commands
        };
        if let Err(e) = renderer.as_mut().render_frame(frame_commands, clear) {
            tracing::error!("render_frame failed: {e}");
        }
        #[cfg(target_os = "android")]
        if let Some(session) = self.hint_session {
            let duration_ns = self.frame_start.elapsed().as_nanos() as std::ffi::c_long;
            unsafe {
                adpf::APerformanceHint_reportActualWorkDuration(session, duration_ns);
            }
        }
    }

    fn on_suspend(&mut self) {
        // Drop the sender to signal the render thread to exit, then wait for it to finish.
        drop(self.render_tx.take());
        if let Some(join) = self.render_join.take() {
            // Reclaim the renderer so the next resume can rebind the surface instead of rebuilding device/pipelines/caches.
            match join.join() {
                Ok(hw) => self.hw_renderer = Some(hw),
                Err(_) => {
                    tracing::warn!("render thread panicked on suspend, renderer lost");
                    self.hw_renderer = None;
                }
            }
        }
        #[cfg(target_os = "android")]
        if let Some(session) = self.hint_session.take() {
            unsafe {
                adpf::APerformanceHint_closeSession(session);
            }
        }
    }

    fn new_events(&mut self) {
        begin_batch();
    }

    fn about_to_wait(&mut self) -> Option<std::time::Duration> {
        end_batch();
        let tree_dirty = self.tree.as_ref().map(|t| t.is_dirty()).unwrap_or(false);
        if tree_dirty {
            // Return the time remaining in the current frame budget so the platform wakes us up exactly when the next 60fps slot opens (or immediately if already past it).
            Some(FRAME_BUDGET.saturating_sub(self.last_frame.elapsed()))
        } else {
            let dev_keepalive = self.dev.keepalive_interval();
            if self.renderer_is_hardware || dev_keepalive.is_some() {
                // Hardware: 1fps minimum to keep the GPU in an active power state; dev plugin: honor its requested keepalive cadence (e.g. FPS counter tick-down).
                Some(dev_keepalive.unwrap_or(std::time::Duration::from_millis(1000)))
            } else {
                None
            }
        }
    }
}

fn hardware_cache_path(app_name: &str, paths: &dyn AppPathsProvider) -> Option<std::path::PathBuf> {
    paths.cache_dir().map(|d| d.join("rsx").join(app_name))
}

fn android_sans_serif_candidates() -> Vec<String> {
    vec![
        "Roboto".to_string(),
        "Droid Sans".to_string(),
        "MiSans Latin".to_string(),
        "Noto Sans".to_string(),
    ]
}

fn build_hardware_font_config(
    font_paths: Vec<std::path::PathBuf>,
    font_data: Vec<Vec<u8>>,
    android: bool,
) -> renderer_text::TextShaperConfig {
    renderer_text::TextShaperConfig {
        font: build_font_config(font_paths, font_data, android),
        ..renderer_text::TextShaperConfig::default()
    }
}

fn build_font_config(
    font_paths: Vec<std::path::PathBuf>,
    font_data: Vec<Vec<u8>>,
    android: bool,
) -> renderer_core::FontConfig {
    renderer_core::FontConfig {
        extra_font_paths: font_paths,
        font_data,
        system_fonts_dir: android.then(|| std::path::PathBuf::from("/system/fonts")),
        sans_serif_family_candidates: if android {
            android_sans_serif_candidates()
        } else {
            vec![]
        },
    }
}

fn build_software_renderer_config(
    font_paths: Vec<std::path::PathBuf>,
    font_data: Vec<Vec<u8>>,
    android: bool,
) -> SoftwareRendererConfig {
    SoftwareRendererConfig {
        font: build_font_config(font_paths, font_data, android),
        ..SoftwareRendererConfig::default()
    }
}

fn spawn_hardware_render_thread<W>(
    renderer: HardwareRenderer<W>,
) -> (
    std::sync::mpsc::SyncSender<HardwareFrameMsg>,
    std::thread::JoinHandle<HardwareRenderer<W>>,
)
where
    W: Window + Clone + Send + Sync + 'static,
{
    let (tx, rx) = std::sync::mpsc::sync_channel::<HardwareFrameMsg>(1);
    let join = std::thread::Builder::new()
        .name("rsx-render".to_string())
        .spawn(move || {
            let mut renderer = renderer;
            let mut current_width = 0u32;
            let mut current_height = 0u32;
            // ADPF lives on THIS thread: create the hint session with the render thread's own TID so reportActualWorkDuration drives the scheduler for the thread that actually submits GPU work. The session handle is not Send, so it is created, used, and closed here and never crosses a thread boundary. (The SW/fallback path keeps its own session on the UI thread.)
            #[cfg(target_os = "android")]
            let hint_session = unsafe {
                let manager = adpf::APerformanceHint_getManager();
                if manager.is_null() {
                    None
                } else {
                    let tid = libc::syscall(libc::SYS_gettid) as i32;
                    let s = adpf::APerformanceHint_createSession(manager, &tid, 1, 16_666_667);
                    if s.is_null() { None } else { Some(s) }
                }
            };
            while let Ok(msg) = rx.recv() {
                // Drop stale frames to stay responsive, but never skip one that resizes the surface: the wgpu surface is reconfigured inside begin_frame, so a dropped resize frame leaves it at the old size and the window shows clipped content or empty margins until the next accepted frame.
                let size_changed = msg.width != current_width || msg.height != current_height;
                if !size_changed && msg.timestamp.elapsed() > FRAME_BUDGET {
                    continue;
                }
                #[cfg(target_os = "android")]
                let frame_start = std::time::Instant::now();
                if renderer
                    .begin_frame(msg.width, msg.height, msg.scale_factor, msg.generation)
                    .is_err()
                {
                    continue;
                }
                current_width = msg.width;
                current_height = msg.height;
                let _ = renderer.render_frame(&msg.commands, msg.clear);
                #[cfg(target_os = "android")]
                if let Some(session) = hint_session {
                    let duration_ns = frame_start.elapsed().as_nanos() as std::ffi::c_long;
                    unsafe {
                        adpf::APerformanceHint_reportActualWorkDuration(session, duration_ns);
                    }
                }
            }
            #[cfg(target_os = "android")]
            if let Some(session) = hint_session {
                unsafe {
                    adpf::APerformanceHint_closeSession(session);
                }
            }
            // Return the renderer so on_suspend can reclaim it and keep warm caches across resume.
            renderer
        })
        .expect("failed to spawn render thread");
    (tx, join)
}

#[cfg(rsx_hot_reload)]
fn apply_dev_window_overrides(config: &mut platform_core::WindowConfig) {
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
            return;
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
            render_join: None,
            hw_renderer: None,
            #[cfg(all(feature = "dev", not(target_os = "android")))]
            hot_reload_rx: None,
        },
    ) {
        tracing::error!("Event loop exited with error: {e}");
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
    apply_dev_window_overrides(&mut window);
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

#[cfg(all(feature = "dev", not(target_os = "android")))]
pub fn run_hot_reload_host(
    lib_path: &str,
    socket_path: &str,
    config: crate::app_config::AppConfig,
    app_name: &str,
) {
    let initial_app = match crate::hot::load_hot_app(std::path::Path::new(lib_path)) {
        Ok(app) => app,
        Err(e) => {
            tracing::error!("failed to load dylib: {e}");
            return;
        }
    };
    let hot_rx = crate::hot::listen_hot_reload(socket_path);
    let paths: Box<dyn services_core::AppPathsProvider> = Box::new(DesktopPathsProvider);
    let prefs = UserPrefs::load(app_name, paths.as_ref());
    let backend = prefs.backend.unwrap_or_else(config::compile_time_backend);
    let platform = match WinitPlatform::try_new() {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Failed to create event loop: {e}");
            return;
        }
    };
    let crate::app_config::AppConfig {
        mut window,
        font_paths,
        font_data,
    } = config;
    #[cfg(rsx_hot_reload)]
    apply_dev_window_overrides(&mut window);
    if let Some(custom) = initial_app.window_config() {
        window = custom;
    }
    if let Err(e) = platform.run(
        window,
        AppHandler::<WinitWindow, rsx_devtools::DevTools> {
            app: Box::new(initial_app),
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
            dev: rsx_devtools::DevTools::default(),
            paths,
            font_paths,
            font_data,
            _window: std::marker::PhantomData,
            render_tx: None,
            render_join: None,
            hw_renderer: None,
            hot_reload_rx: Some(hot_rx),
        },
    ) {
        tracing::error!("Event loop error: {e}");
    }
}
