use devtools_core::{DevAction, DevPlugin};
use platform_core::{Event, EventHandler, Window};
use reactive_core::{FlushNotifyHandle, begin_batch, end_batch, set_flush_notify};
use renderer_core::{RenderBackend, RendererError};
use renderer_hardware::{HardwareRenderer, HardwareRendererConfig};
use renderer_software::SoftwareRenderer;
use services_core::AppPathsProvider;
use ui_core::{ComponentList, EventResult};

use crate::app::App;
use crate::config::{self, RendererBackend};
use crate::prefs::UserPrefs;
use crate::window_signals::WindowSignals;

use super::FRAME_BUDGET;
use super::font_config::{
    build_font_config, build_hardware_font_config, build_software_renderer_config,
    hardware_cache_path,
};
use super::hot_host::{HardwareFrameMsg, spawn_hardware_render_thread};

pub(super) struct AppHandler<W, D: DevPlugin>
where
    W: Window + Clone + Send + Sync + 'static,
{
    pub(super) app: Box<dyn App>,
    pub(super) tree: Option<ComponentList>,
    pub(super) renderer: Option<Box<dyn RenderBackend>>,
    pub(super) renderer_is_hardware: bool,
    pub(super) backend: RendererBackend,
    pub(super) prefs: UserPrefs,
    pub(super) paths: Box<dyn AppPathsProvider>,
    pub(super) pending_restart: bool,
    pub(super) pending_renderer:
        Option<std::thread::JoinHandle<Result<HardwareRenderer<W>, RendererError>>>,
    pub(super) _flush_notify: Option<FlushNotifyHandle>,
    pub(super) scale_factor: f32,
    // Reused across frames so the SW/HiDPI command scaling allocates neither a fresh Vec nor redundant per-command style Arcs.
    pub(super) scale_scratch: renderer_core::ScaleScratch,
    pub(super) window_signals: Option<WindowSignals>,
    pub(super) app_name: String,
    pub(super) last_frame: std::time::Instant,
    pub(super) dev: D,
    pub(super) font_paths: Vec<std::path::PathBuf>,
    pub(super) font_data: Vec<Vec<u8>>,
    pub(super) _window: std::marker::PhantomData<W>,
    pub(super) render_tx: Option<std::sync::mpsc::SyncSender<HardwareFrameMsg>>,
    pub(super) render_join: Option<std::thread::JoinHandle<HardwareRenderer<W>>>,
    pub(super) hw_renderer: Option<HardwareRenderer<W>>,
    #[cfg(all(feature = "dev", not(target_os = "android")))]
    pub(super) hot_reload_rx: Option<std::sync::mpsc::Receiver<crate::hot::HotEvent>>,
    #[cfg(target_os = "android")]
    pub(super) hint_session: Option<*mut std::ffi::c_void>,
    #[cfg(target_os = "android")]
    pub(super) frame_start: std::time::Instant,
}

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
                let manager = super::android::adpf::APerformanceHint_getManager();
                if manager.is_null() {
                    None
                } else {
                    let tid = libc::syscall(libc::SYS_gettid) as i32;
                    let s = super::android::adpf::APerformanceHint_createSession(
                        manager, &tid, 1, 16_666_667,
                    );
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
                                // Carry serializable hot state into the incoming dylib while the old tree (and its signals) is still alive; hot_signal consumes it as components remount.
                                if let Some(blob) = self.app.hot_snapshot() {
                                    new_app.hot_restore(&blob);
                                }
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
        // Drive the motion engine before tree_dirty is read below: tick()'s .set() calls only enqueue effects while a batch is open (new_events already opened one), so force a flush here to re-run any segment reading an animated value now, not on the next cycle. This is what makes an animation-only frame (no user event, tree otherwise clean) observe interpolated values in this same frame's tree.commands().
        self.app.motion_tick(std::time::Instant::now());
        end_batch();
        begin_batch();

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
                super::android::adpf::APerformanceHint_reportActualWorkDuration(
                    session,
                    duration_ns,
                );
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
                super::android::adpf::APerformanceHint_closeSession(session);
            }
        }
    }

    fn new_events(&mut self) {
        begin_batch();
    }

    fn about_to_wait(&mut self) -> Option<std::time::Duration> {
        end_batch();
        let tree_dirty = self.tree.as_ref().map(|t| t.is_dirty()).unwrap_or(false);
        // An unsettled animation must keep the loop scheduling frames even while the tree itself is momentarily clean (e.g. the tick that only established t0); once it settles, has_active() drops out and this falls through to the existing idle/keepalive branch below.
        if tree_dirty || self.app.motion_has_active() {
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
