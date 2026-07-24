use devtools_core::{DevAction, DevPlugin};
use platform_core::{Event, EventHandler, Window, WindowCommand};
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

use super::font_config::{
    SystemFonts, build_font_config, build_hardware_font_config, build_software_renderer_config,
    hardware_cache_path,
};
use super::hot_host::{HardwareFrameMsg, spawn_hardware_render_thread};
use super::{FRAME_BUDGET, HW_KEEPALIVE_GRACE};

pub(super) struct AppHandler<W, D: DevPlugin>
where
    W: Window + Clone + Send + Sync + 'static,
{
    pub(super) app: Box<dyn App>,
    // This handler's surface world, activated around every lifecycle call so its build/event/frame resolve
    // layout/overlay/focus against the right surface. `None` for a single-window app: its ambient thread-local
    // world IS its one surface, and entering it is a no-op. The multi-surface runner injects `Some` per window.
    pub(super) surface: Option<std::rc::Rc<ui_core::Surface>>,
    // This window's OS command queue (Close/Drag/SetTitle/…), entered alongside `surface` so a title-bar
    // action pushed by one window's widgets targets that window — never a sibling sharing the M3 UI thread.
    // Only entered when `surface` is `Some`; a single-window app keeps using the ambient thread-local queue.
    pub(super) window_commands: platform_core::WindowCommandContext,
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
    // Set when the app pushed WindowCommand::Close (a custom title-bar close button); polled by the platform
    // via take_exit_request to leave the run loop.
    pub(super) exit_requested: bool,
    // A Send/Sync handle that wakes this window's loop, built from the window at resume and handed to app code
    // via AppCtx so background threads can request a redraw when their results are ready.
    pub(super) redraw_waker: Option<crate::app_context::RedrawWaker>,
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
    // F2: command buffers recycled back from the render thread, plus a small free-list the send path
    // refills instead of allocating a fresh Vec each frame.
    pub(super) render_ret_rx: Option<std::sync::mpsc::Receiver<Vec<renderer_core::DrawCommand>>>,
    pub(super) command_buf_pool: Vec<Vec<renderer_core::DrawCommand>>,
    pub(super) hw_renderer: Option<HardwareRenderer<W>>,
    #[cfg(all(feature = "dev", not(target_os = "android")))]
    pub(super) hot_reload_rx: Option<std::sync::mpsc::Receiver<crate::hot::HotEvent>>,
    #[cfg(target_os = "android")]
    pub(super) hint_session: Option<platform_android::AdpfSession>,
    #[cfg(target_os = "android")]
    pub(super) frame_start: std::time::Instant,
}

// Assembles an `AppHandler` from its ready inputs — the one place the large field literal lives, shared by the
// single-surface `run_with_platform` and the per-surface handler factory in `run_multi_with_platform`. Builds
// no renderer and touches no thread-local state, so it is safe to call on whatever thread will later drive the
// handler (e.g. a per-surface worker thread).
#[allow(clippy::too_many_arguments)]
pub(super) fn build_app_handler<W, D>(
    app: Box<dyn App>,
    paths: Box<dyn AppPathsProvider>,
    font_paths: Vec<std::path::PathBuf>,
    font_data: Vec<Vec<u8>>,
    backend: crate::config::RendererBackend,
    prefs: UserPrefs,
    app_name: String,
) -> AppHandler<W, D>
where
    W: Window + Clone + Send + Sync + 'static,
    D: DevPlugin,
{
    AppHandler::<W, D> {
        app,
        surface: None,
        window_commands: platform_core::WindowCommandContext::new(),
        tree: None,
        renderer: None,
        renderer_is_hardware: false,
        backend,
        prefs,
        pending_restart: false,
        pending_renderer: None,
        _flush_notify: None,
        scale_factor: 1.0,
        exit_requested: false,
        redraw_waker: None,
        scale_scratch: renderer_core::ScaleScratch::new(),
        window_signals: None,
        app_name,
        last_frame: std::time::Instant::now(),
        dev: D::default(),
        paths,
        font_paths,
        font_data,
        _window: std::marker::PhantomData,
        render_tx: None,
        render_ret_rx: None,
        command_buf_pool: Vec::new(),
        render_join: None,
        hw_renderer: None,
        #[cfg(all(feature = "dev", not(target_os = "android")))]
        hot_reload_rx: None,
    }
}

// Holds a surface's reactive/layout world and its OS command queue active together for one lifecycle call.
// Dropped fields restore the previous surface and queue; the two restores are independent, so drop order is
// irrelevant.
struct LifecycleGuard {
    _surface: ui_core::SurfaceGuard,
    _commands: platform_core::WindowCommandGuard,
}

impl<W, D> AppHandler<W, D>
where
    W: Window + Clone + Send + Sync + 'static,
    D: DevPlugin,
{
    /// Enters this handler's surface world for the duration of a lifecycle call, so its build/event/frame
    /// resolve layout/overlay/focus (and the reactive current-surface) against the right surface. Returns
    /// `None` for a single-window app — its ambient world is its one surface — making this a zero-cost no-op.
    /// The returned guard owns the restore state and does not borrow `self`, so callers can mutate `self`
    /// while it is held (`let _surface = self.enter_surface();`).
    // Drains and applies the window-management commands a handler enqueued — from a title-bar control during
    // event dispatch, or from `on_frame` (e.g. raising this window on a routed handoff). Returns whether any
    // applied. Routed through the App bridge so the dylib-backed `HotApp` drains the dylib's own queue.
    fn apply_window_commands(&mut self, window: &W) -> bool {
        let mut applied = false;
        for cmd in self.app.drain_window_commands() {
            applied = true;
            match cmd {
                WindowCommand::Drag => window.drag_window(),
                WindowCommand::Minimize => window.set_minimized(true),
                WindowCommand::ToggleMaximize => window.set_maximized(!window.is_maximized()),
                WindowCommand::SetMaximized(v) => window.set_maximized(v),
                WindowCommand::SetTitle(title) => window.set_title(&title),
                WindowCommand::Focus => window.focus_window(),
                WindowCommand::Close => self.exit_requested = true,
            }
        }
        applied
    }

    fn enter_surface(&self) -> Option<LifecycleGuard> {
        self.surface.as_ref().map(|s| LifecycleGuard {
            _surface: s.enter(),
            _commands: self.window_commands.enter(),
        })
    }

    /// Whether the app asked for a transparent surface (`WindowConfig::is_transparent`). Read at each renderer creation so hardware picks a premultiplied-alpha composite mode and software presents an alpha-preserving buffer.
    fn is_transparent(&self) -> bool {
        self.app
            .window_config()
            .map(|c| c.is_transparent)
            .unwrap_or(false)
    }

    // Builds the configured on-screen renderer (software, or hardware with an auto→software fallback) and wires
    // up the render thread for the hardware path. Returns false if renderer creation failed. Split out of
    // on_resume so the offscreen/headless path (which needs no surface) can bypass it entirely.
    fn init_windowed_renderer(&mut self, window: &W, system_fonts: &SystemFonts) -> bool {
        let android = cfg!(target_os = "android");
        let cache_path = hardware_cache_path(&self.app_name, self.paths.as_ref());
        match self.backend {
            RendererBackend::Software => {
                let budget = build_software_renderer_config(
                    self.font_paths.clone(),
                    self.font_data.clone(),
                    system_fonts,
                    self.is_transparent(),
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
                    system_fonts,
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
                        HardwareRendererConfig {
                            transparent: self.is_transparent(),
                            ..HardwareRendererConfig::default()
                        },
                    )
                };
                match hw_result {
                    Ok(hw) => {
                        let (tx, ret_rx, join) = spawn_hardware_render_thread(hw);
                        self.render_tx = Some(tx);
                        self.render_ret_rx = Some(ret_rx);
                        self.render_join = Some(join);
                        self.renderer_is_hardware = true;
                    }
                    Err(e) if matches!(self.backend, RendererBackend::Auto) => {
                        tracing::warn!("HW renderer unavailable ({e}), falling back to SW");
                        let budget = build_software_renderer_config(
                            self.font_paths.clone(),
                            self.font_data.clone(),
                            system_fonts,
                            self.is_transparent(),
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
        true
    }
}

impl<W, D> EventHandler<W> for AppHandler<W, D>
where
    W: Window + Clone + Send + Sync + 'static,
    D: DevPlugin,
{
    fn on_resume(&mut self, window: &W) -> bool {
        let _surface = self.enter_surface();
        let system_fonts = SystemFonts::from_provider(self.paths.as_ref());
        // Point the layout-time text measurer at the same fonts as the renderer, on this (the layout) thread, before any layout runs. Otherwise it falls back to system defaults and aborts on Android ("no default font found").
        renderer_text::set_measure_font_config(build_font_config(
            self.font_paths.clone(),
            self.font_data.clone(),
            &system_fonts,
        ));
        // Offscreen/headless windows have no surface, so a windowed renderer can't create one: rasterize into a
        // CPU pixmap (read back via `last_frame_rgba`), forced regardless of the configured backend so the
        // headless path needs no GPU adapter. On-screen windows build the configured renderer.
        let renderer_ok = if window.is_offscreen() {
            let budget = build_software_renderer_config(
                self.font_paths.clone(),
                self.font_data.clone(),
                &system_fonts,
                self.is_transparent(),
            );
            self.renderer = Some(Box::new(SoftwareRenderer::<W, W>::new_headless(
                window.width(),
                window.height(),
                budget,
            )));
            true
        } else {
            self.init_windowed_renderer(window, &system_fonts)
        };
        if !renderer_ok {
            return false;
        }
        let sf = window.scale_factor() as f32;
        self.scale_factor = sf;
        self.window_signals = Some(WindowSignals::new(
            window.width() as f32 / sf,
            window.height() as f32 / sf,
        ));
        // Prefer the process-global loop wake (installed by the platform): it wakes the loop — redrawing every
        // surface — without holding any window, so an app can cache this waker or hand it to a worker thread
        // and, if its content is later moved to another window, the original still closes and wakeups still
        // reach it. Fall back to a window-cloning wake on backends that install no loop waker.
        self.redraw_waker = Some(match platform_core::loop_waker() {
            Some(wake) => crate::app_context::RedrawWaker::new(move || wake()),
            None => {
                let window = window.clone();
                crate::app_context::RedrawWaker::new(move || window.request_redraw())
            }
        });
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
            self.hint_session = platform_android::AdpfSession::new(16_666_667, None);
        }
        window.request_redraw();
        true
    }

    fn on_event(&mut self, event: Event, window: &W) {
        let _surface = self.enter_surface();
        if let Event::ScaleFactorChanged { scale_factor } = &event {
            self.scale_factor = *scale_factor as f32;
        }
        if let Event::WindowResized { width, height } = &event {
            if let Some(ref signals) = self.window_signals {
                signals.update(*width as f32, *height as f32);
            }
        }
        if let Event::ColorSchemeChanged { dark } = &event {
            // Drives the follow_system effect (which writes the theme signal); batch the app's runtime so the
            // re-render flushes cleanly across the hot-reload boundary. No widget consumes this event.
            self.app.begin_event_batch();
            self.app.set_system_dark(*dark);
            self.app.end_event_batch();
            window.request_redraw();
            return;
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
                            let system_fonts = SystemFonts::from_provider(self.paths.as_ref());
                            // Computed before the closure: `self` is not `Send`, so its transparency must be captured by value, not read across the spawn.
                            let transparent = self.is_transparent();
                            let handle = std::thread::spawn(move || {
                                let font_config = build_hardware_font_config(
                                    font_paths,
                                    font_data,
                                    &system_fonts,
                                );
                                HardwareRenderer::new(
                                    window_clone,
                                    cache_path.as_deref(),
                                    android,
                                    font_config,
                                    HardwareRendererConfig {
                                        transparent,
                                        ..HardwareRendererConfig::default()
                                    },
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
        // Batch the app's OWN reactive runtime across dispatch. In hot-reload the app dylib links its own
        // reactive-core copy (separate runtime), which the host's begin/end_batch cannot reach; a handler's
        // signal write would then flush immediately and re-run a segment's effect while its widget is still
        // borrowed for on_event, silently dropping that segment's subscriptions. Closing the batch after
        // dispatch (every borrow released) makes the deferred effects flush safely. No-op for a normal app.
        self.app.begin_event_batch();
        // Overlays (modals/dropdowns) paint on top, so a positioned pointer event over one must reach it
        // FIRST and be blocked from the content behind. The overlay registry lives on the app's side of the
        // hot-reload boundary (where `overlay` widgets register), so consult it via the App bridge before
        // the tree walk; when an overlay consumes the event, skip the walk entirely (this is the block).
        let handled = if self.app.dispatch_overlays(&event) {
            EventResult::Handled
        } else {
            self.tree
                .as_mut()
                .map(|tree| tree.on_event(&event))
                .unwrap_or(EventResult::Ignored)
        };
        self.app.end_event_batch();
        // Apply any window-management commands a handler enqueued this dispatch (custom title-bar controls).
        // Drag must run inside the pointer-press dispatch it originated from, so this sits right after the walk.
        let window_command_applied = self.apply_window_commands(window);
        if handled == EventResult::Handled || window_command_applied {
            #[cfg(feature = "dev")]
            if let Some(tree) = &self.tree {
                tree.bump_force_ticks();
            }
            // Flush reactive effects immediately so on_redraw() in the same cycle finds tree_dirty=true rather than deferring to the next cycle.
            end_batch();
            begin_batch();
            window.request_redraw();
        }
    }

    fn on_redraw(&mut self, window: &W) {
        let _surface = self.enter_surface();
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
        // Runtime-driven relayout: a reactive change (e.g. a reactive list adding/removing items) mutated
        // the layout tree during the flush above but the app shell only recomputes layout on resize/route
        // changes. Re-lay out any dirtied root here — outside a batch, so the rect updates it produces flush
        // their segment effects before tree_dirty is read below and the frame is composed. Routed through
        // the app so the dylib-backed `HotApp` relayouts the dylib's runtime (where the tree lives), not the
        // host's empty one.
        self.app.relayout();
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
                redraw_waker: self.redraw_waker.as_ref(),
                raw_window_handle: raw_window_handle::HasWindowHandle::window_handle(window)
                    .ok()
                    .map(|h| h.as_raw()),
                raw_display_handle: raw_window_handle::HasDisplayHandle::display_handle(window)
                    .ok()
                    .map(|h| h.as_raw()),
            };
            self.app.on_frame(&mut ctx);
        }
        // Apply commands enqueued during on_frame (e.g. raising this window on a routed handoff): on_event's
        // drain only runs on input events, so a frame-driven command would otherwise wait for the next one.
        self.apply_window_commands(window);
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
                        let (tx, ret_rx, join) = spawn_hardware_render_thread(new_renderer);
                        self.render_tx = Some(tx);
                        self.render_ret_rx = Some(ret_rx);
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
            let system_fonts = SystemFonts::from_provider(self.paths.as_ref());
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
                        &system_fonts,
                        self.is_transparent(),
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
                        &system_fonts,
                    );
                    match HardwareRenderer::new(
                        window.clone(),
                        cache_path.as_deref(),
                        android,
                        font_config,
                        HardwareRendererConfig {
                            transparent: self.is_transparent(),
                            ..HardwareRendererConfig::default()
                        },
                    ) {
                        Ok(hw) => {
                            let (tx, ret_rx, join) = spawn_hardware_render_thread(hw);
                            self.render_tx = Some(tx);
                            self.render_ret_rx = Some(ret_rx);
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
            // F2: reclaim buffers the render thread finished with, capped so the free-list stays tiny.
            if let Some(rx) = &self.render_ret_rx {
                while let Ok(buf) = rx.try_recv() {
                    if self.command_buf_pool.len() < 3 {
                        self.command_buf_pool.push(buf);
                    }
                }
            }
            let build_start = renderer_core::perf::now_if_enabled();
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
            renderer_core::perf::record_since(renderer_core::perf::Phase::Build, build_start);
            let clone_start = renderer_core::perf::now_if_enabled();
            // F2: refill a recycled buffer instead of allocating a fresh Vec every frame.
            let mut commands = self.command_buf_pool.pop().unwrap_or_default();
            commands.clear();
            commands.extend_from_slice(&frame_commands);
            renderer_core::perf::record_since(renderer_core::perf::Phase::Clone, clone_start);
            let msg = HardwareFrameMsg {
                width: w,
                height: h,
                scale_factor: self.scale_factor,
                generation: self.tree.as_ref().map(|t| t.generation()).unwrap_or(0),
                commands,
                clear,
                timestamp: std::time::Instant::now(),
            };
            // Drop frame if render thread is busy; keeps the main thread responsive. On a dropped or
            // disconnected send, recover the buffer for the free-list instead of freeing it.
            if let Err(e) = tx.try_send(msg) {
                let recovered = match e {
                    std::sync::mpsc::TrySendError::Full(m)
                    | std::sync::mpsc::TrySendError::Disconnected(m) => m.commands,
                };
                if self.command_buf_pool.len() < 3 {
                    self.command_buf_pool.push(recovered);
                }
            }
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
        renderer_core::perf::tick();
        let build_start = renderer_core::perf::now_if_enabled();
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
        renderer_core::perf::record_since(renderer_core::perf::Phase::Build, build_start);
        let gpu_start = renderer_core::perf::now_if_enabled();
        if let Err(e) = renderer.as_mut().render_frame(frame_commands, clear) {
            tracing::error!("render_frame failed: {e}");
        }
        renderer_core::perf::record_since(renderer_core::perf::Phase::Gpu, gpu_start);
        #[cfg(target_os = "android")]
        if let Some(session) = &self.hint_session {
            let duration_ns = self.frame_start.elapsed().as_nanos() as i64;
            session.report(duration_ns);
        }
    }

    fn on_suspend(&mut self) {
        let _surface = self.enter_surface();
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
        // Dropping the session runs closeSession (on this thread, matching the TID it was created with).
        #[cfg(target_os = "android")]
        {
            self.hint_session = None;
        }
    }

    fn new_events(&mut self) {
        begin_batch();
    }

    fn take_exit_request(&mut self) -> bool {
        std::mem::take(&mut self.exit_requested)
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
            if let Some(interval) = dev_keepalive {
                // Dev plugin drives its own cadence (e.g. FPS counter tick-down).
                Some(interval)
            } else if self.renderer_is_hardware && self.last_frame.elapsed() < HW_KEEPALIVE_GRACE {
                // F4: hold the GPU in an active power state at 1fps for a short grace window after the
                // last content frame (covers interactive bursts), then let it sleep — real input/redraw
                // events still wake the loop. `last_frame` isn't reset by keepalive blits, so its
                // elapsed measures true inactivity. Saves ~1 idle GPU wake/sec on battery.
                Some(std::time::Duration::from_millis(1000))
            } else {
                None
            }
        }
    }

    // Hands back the offscreen renderer's last frame so a headless platform can read pixels. Only the
    // windowless software renderer holds a readable pixmap; the HW/windowed paths present and return None.
    fn last_frame_rgba(&self) -> Option<Vec<u8>> {
        self.renderer.as_ref().and_then(|r| r.read_rgba())
    }
}
