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
use ui_core::ComponentList;

struct HwFrameMsg {
    w: u32,
    h: u32,
    sf: f32,
    commands: Vec<renderer_core::DrawCommand>,
    clear: Option<renderer_core::Color>,
    at: std::time::Instant,
}

use rsx_devtools::{DevAction, DevPlugin};

use crate::app::App;
use crate::app_config::AppConfig;
use crate::config::{self, RendererBackend};
use crate::prefs::UserPrefs;
use crate::window_signals::WindowSignals;

#[cfg(all(feature = "runtime", not(target_os = "android")))]
use crate::paths::DesktopPathsProvider;
#[cfg(not(target_os = "android"))]
use platform_winit::{WinitPlatform, WinitWindow};
use renderer_hardware::HardwareRenderer;

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
    window_signals: Option<WindowSignals>,
    app_name: String,
    last_frame: std::time::Instant,
    dev: D,
    font_paths: Vec<std::path::PathBuf>,
    font_data: Vec<Vec<u8>>,
    _window: std::marker::PhantomData<W>,
    render_tx: Option<std::sync::mpsc::SyncSender<HwFrameMsg>>,
    render_join: Option<std::thread::JoinHandle<()>>,
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
        let cache_path = hardware_cache_path(&self.app_name, self.paths.as_ref());
        match self.backend {
            RendererBackend::Software => {
                let budget =
                    build_sw_budget(self.font_paths.clone(), self.font_data.clone(), android);
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
                let font_config =
                    build_hw_font_config(self.font_paths.clone(), self.font_data.clone(), android);
                match HardwareRenderer::new(
                    window.clone(),
                    cache_path.as_deref(),
                    android,
                    font_config,
                ) {
                    Ok(hw) => {
                        let (tx, join) = spawn_hw_render_thread(hw);
                        self.render_tx = Some(tx);
                        self.render_join = Some(join);
                        self.renderer_is_hardware = true;
                    }
                    Err(e) if matches!(self.backend, RendererBackend::Auto) => {
                        tracing::warn!("HW renderer unavailable ({e}), falling back to SW");
                        let budget = build_sw_budget(
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
        // Synthesize an initial WindowResized so apps that initialize layout from that event
        // start with the correct logical dimensions instead of their hardcoded defaults.
        let initial_resize = platform_core::Event::WindowResized {
            width: (window.width() as f32 / sf) as u32,
            height: (window.height() as f32 / sf) as u32,
        };
        if let Some(ref mut tree) = self.tree {
            tree.on_event(&initial_resize);
        }

        let w = window.clone();
        self._flush_notify = Some(set_flush_notify(move || w.request_redraw()));
        #[cfg(target_os = "android")]
        {
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
                                    build_hw_font_config(font_paths, font_data, android);
                                HardwareRenderer::new(
                                    window_clone,
                                    cache_path.as_deref(),
                                    android,
                                    font_config,
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
            tree.on_event(&event);
        }
    }

    fn on_redraw(&mut self, window: &W) {
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
                        let (tx, join) = spawn_hw_render_thread(new_renderer);
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
                    let budget =
                        build_sw_budget(self.font_paths.clone(), self.font_data.clone(), android);
                    match SoftwareRenderer::new(window.clone(), window.clone(), budget) {
                        Ok(r) => {
                            self.renderer = Some(Box::new(r));
                            self.renderer_is_hardware = false;
                        }
                        Err(e) => tracing::error!("Failed to switch to SW renderer: {e}"),
                    }
                }
                RendererBackend::Hardware | RendererBackend::Auto => {
                    let font_config = build_hw_font_config(
                        self.font_paths.clone(),
                        self.font_data.clone(),
                        android,
                    );
                    match HardwareRenderer::new(
                        window.clone(),
                        cache_path.as_deref(),
                        android,
                        font_config,
                    ) {
                        Ok(hw) => {
                            let (tx, join) = spawn_hw_render_thread(hw);
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
            let logical_w = w as f32 / self.scale_factor;
            let logical_h = h as f32 / self.scale_factor;
            let frame_commands = self
                .dev
                .on_frame(base_slice, logical_w, logical_h, tree_dirty);
            let commands: Vec<renderer_core::DrawCommand> = if self.scale_factor != 1.0 {
                renderer_core::scale_commands(&frame_commands, self.scale_factor)
                    .unwrap_or_default()
            } else {
                frame_commands.to_vec()
            };
            let msg = HwFrameMsg {
                w,
                h,
                sf: self.scale_factor,
                commands,
                clear,
                at: std::time::Instant::now(),
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
        if let Err(e) = renderer.begin_frame(w, h, self.scale_factor) {
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
        let logical_w = w as f32 / self.scale_factor;
        let logical_h = h as f32 / self.scale_factor;
        let frame_commands = self
            .dev
            .on_frame(base_slice, logical_w, logical_h, tree_dirty);
        let frame_commands: &[renderer_core::DrawCommand] = &frame_commands;
        let scaled_storage: Vec<renderer_core::DrawCommand>;
        let frame_commands = if self.scale_factor != 1.0 {
            scaled_storage = renderer_core::scale_commands(frame_commands, self.scale_factor)
                .unwrap_or_default();
            &scaled_storage
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
            let _ = join.join();
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

fn build_hw_font_config(
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

fn build_sw_budget(
    font_paths: Vec<std::path::PathBuf>,
    font_data: Vec<Vec<u8>>,
    android: bool,
) -> SoftwareRendererConfig {
    SoftwareRendererConfig {
        font: build_font_config(font_paths, font_data, android),
        ..SoftwareRendererConfig::default()
    }
}

fn spawn_hw_render_thread<W>(
    renderer: HardwareRenderer<W>,
) -> (
    std::sync::mpsc::SyncSender<HwFrameMsg>,
    std::thread::JoinHandle<()>,
)
where
    W: Window + Clone + Send + Sync + 'static,
{
    let (tx, rx) = std::sync::mpsc::sync_channel::<HwFrameMsg>(1);
    let join = std::thread::Builder::new()
        .name("rsx-render".to_string())
        .spawn(move || {
            let mut renderer = renderer;
            while let Ok(msg) = rx.recv() {
                if msg.at.elapsed() > FRAME_BUDGET {
                    continue;
                }
                if renderer.begin_frame(msg.w, msg.h, msg.sf).is_err() {
                    continue;
                }
                let _ = renderer.render_frame(&msg.commands, msg.clear);
            }
        })
        .expect("failed to spawn render thread");
    (tx, join)
}

#[cfg(not(target_os = "android"))]
fn run_with_plugin<A: App, D: DevPlugin>(config: AppConfig, app: A, app_name: &str) {
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
        window,
        font_paths,
        font_data,
    } = config;
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
        },
    ) {
        tracing::error!("Event loop exited with error: {e}");
    }
}

#[cfg(not(target_os = "android"))]
pub fn run_app_with_name<A: App>(config: AppConfig, app: A, app_name: &str) {
    #[cfg(feature = "dev")]
    run_with_plugin::<A, rsx_devtools::DevTools>(config, app, app_name);
    #[cfg(not(feature = "dev"))]
    run_with_plugin::<A, ()>(config, app, app_name);
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

    let platform = match AndroidPlatform::new(android_app) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Failed to create Android event loop: {e}");
            return;
        }
    };
    let AppConfig {
        window,
        font_paths,
        font_data,
    } = config;
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
            hint_session: None,
            frame_start: std::time::Instant::now(),
        },
    ) {
        tracing::error!("Android event loop exited with error: {e}");
    }
}
