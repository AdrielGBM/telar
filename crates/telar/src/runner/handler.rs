use crate::dev_plugin::{DevAction, DevPlugin};
use platform_core::{Event, EventHandler, Window, WindowCommand};
use reactive_core::{FlushNotifyHandle, begin_batch, end_batch, set_flush_notify};
use renderer_core::RenderBackend;
use services_core::AppPathsProvider;
use std::sync::Arc;
use ui_core::EventResult;

use crate::app::App;
use crate::config::{self, RendererBackend};
use crate::prefs::UserPrefs;

use super::COMMAND_BUF_POOL_CAP;
use super::font_config::{SystemFonts, build_font_config};
use super::frame_thread::FrameMsg;
use super::host::{RawHandles, RendererHost, RendererRequest, RendererStart, SurfaceRenderer};
use super::{FRAME_BUDGET, HW_KEEPALIVE_INTERVAL, IDLE_GRACE};

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
    // The app's mounted UI — obtained from the app rather than built here, because under hot reload the tree has
    // to live in the dylib's runtime for its segments to subscribe to anything (see `crate::tree`).
    pub(super) tree: Option<Box<dyn crate::tree::UiTree>>,
    // The offscreen renderer, driven inline on this thread. `None` for an on-screen surface, whose renderer lives on the render thread the host owns.
    pub(super) renderer: Option<Box<dyn RenderBackend>>,
    // Who builds this surface's renderer and keeps it running; behind `dyn` because the frame loop must work for a renderer it cannot name (see `super::host`).
    pub(super) renderer_host: Box<dyn RendererHost<W>>,
    // How this window answers for its OS handles, or `None` when it has none: `platform_core::Window` promises none, so the entry point that knows the window type supplies this.
    pub(super) raw_handles: Option<RawHandles<W>>,
    // Whether the live renderer wants frames while the screen is idle, which the host reports at every start.
    pub(super) renderer_keepalive: bool,
    /// Whether this window has the keyboard. The keepalive rides on it: somebody who can type is somebody
    /// whose next frame should not wait for the GPU to wake up.
    pub(super) focused: bool,
    pub(super) last_input: std::time::Instant,
    // The transparency the live renderer was built for. Only `remount` reads it: an app whose surface changes
    // from opaque to transparent between two mounts needs the renderer built again, and asking the app after
    // the rebuild would compare the new answer with itself.
    pub(super) renderer_transparent: bool,
    pub(super) generation: FrameGeneration,
    pub(super) backend: RendererBackend,
    pub(super) prefs: UserPrefs,
    pub(super) paths: Arc<dyn AppPathsProvider>,
    pub(super) pending_restart: bool,
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
    pub(super) app_name: String,
    pub(super) last_frame: std::time::Instant,
    // When the frame pass last ran, as opposed to `last_frame`, which only advances on a frame carrying new content; `on_redraw` paces itself against this so the pass costs the same however often the platform calls it.
    pub(super) last_tick: std::time::Instant,
    // When a frame was last submitted, content or keepalive; paces the keepalive blit.
    pub(super) last_submit: std::time::Instant,
    pub(super) dev: D,
    pub(super) font_paths: Vec<std::path::PathBuf>,
    pub(super) font_data: Vec<Vec<u8>>,
    pub(super) font_family: Option<String>,
    pub(super) _window: std::marker::PhantomData<W>,
    // F2: a small free-list the send path refills with the buffers the render thread hands back, instead of allocating a fresh Vec each frame.
    pub(super) command_buf_pool: Vec<Vec<renderer_core::DrawCommand>>,
    /// Just the text of the last frame, kept so the accessibility tree can be built when it is asked for
    /// rather than on every frame. A control is named by the text drawn inside it, and that is the only part
    /// of a frame the naming needs — a handful of commands, held by refcounted `Arc<str>`.
    pub(super) frame_text: Vec<renderer_core::DrawCommand>,
    #[cfg(all(feature = "dev", not(target_os = "android")))]
    pub(super) hot_reload_rx: Option<std::sync::mpsc::Receiver<crate::hot::HotEvent>>,
}

/// The number a renderer compares against the last one it drew, and the whole of its monotonicity.
///
/// A renderer skips its pipeline and re-presents the texture it retained when the generation it is handed
/// matches the last one it rendered — so equal generations have to mean identical draw commands, and the
/// number must never go backwards. Three loose `u64` fields written from three places said that only by
/// convention, and disagreed on overflow: one used `+ 1`, the others saturated.
///
/// Two things break the invariant on their own. A remount: the compose counter lives on the tree, so a new
/// tree starts over and a surface whose content never changes hands out the number it did before — which is
/// why `restart` steps past everything already drawn. And a continuous region: its commands are identical
/// every frame while the picture they point at is not, so `next` steps per frame while one is alive.
#[derive(Default)]
pub(super) struct FrameGeneration {
    base: u64,
    last: u64,
    continuous_frames: u64,
}

impl FrameGeneration {
    /// Starts the next tree past every generation this surface has already handed out.
    fn restart(&mut self) {
        self.base = self.last.saturating_add(1);
    }

    /// This frame's generation, and the only place the three counters move.
    fn next(&mut self, composed: u64, continuous: bool) -> u64 {
        if continuous {
            self.continuous_frames = self.continuous_frames.saturating_add(1);
        }
        let generation = self
            .base
            .saturating_add(composed)
            .saturating_add(self.continuous_frames);
        self.last = self.last.max(generation);
        generation
    }
}

// Assembles an `AppHandler` from its ready inputs — the one place the large field literal lives, shared by the
// single-surface `run_with_platform` and the per-surface handler factory in `run_multi_with_platform`. Builds
// no renderer and touches no thread-local state, so it is safe to call on whatever thread will later drive the
// handler (e.g. a per-surface worker thread).
#[allow(clippy::too_many_arguments)]
pub(super) fn build_app_handler<W, D>(
    app: Box<dyn App>,
    paths: Arc<dyn AppPathsProvider>,
    font_paths: Vec<std::path::PathBuf>,
    font_data: Vec<Vec<u8>>,
    font_family: Option<String>,
    backend: crate::config::RendererBackend,
    prefs: UserPrefs,
    app_name: String,
    renderer: SurfaceRenderer<W>,
) -> AppHandler<W, D>
where
    W: Window + Clone + Send + Sync + 'static,
    D: DevPlugin,
{
    let SurfaceRenderer { host, raw_handles } = renderer;
    AppHandler::<W, D> {
        app,
        surface: None,
        window_commands: platform_core::WindowCommandContext::new(),
        tree: None,
        renderer: None,
        renderer_host: host,
        raw_handles,
        renderer_keepalive: false,
        // A platform that never reports focus is one whose window is the only thing on screen, so being
        // believed focused is both the safe answer and the true one.
        focused: true,
        last_input: std::time::Instant::now(),
        renderer_transparent: false,
        generation: FrameGeneration::default(),
        backend,
        prefs,
        pending_restart: false,
        _flush_notify: None,
        scale_factor: 1.0,
        exit_requested: false,
        redraw_waker: None,
        scale_scratch: renderer_core::ScaleScratch::new(),
        app_name,
        last_frame: std::time::Instant::now(),
        // Backdated so the first `on_redraw` after resume composes immediately instead of waiting out a frame it has nothing to pace against yet.
        last_tick: std::time::Instant::now()
            .checked_sub(FRAME_BUDGET)
            .unwrap_or_else(std::time::Instant::now),
        last_submit: std::time::Instant::now()
            .checked_sub(HW_KEEPALIVE_INTERVAL)
            .unwrap_or_else(std::time::Instant::now),
        dev: D::default(),
        paths,
        font_paths,
        font_data,
        font_family,
        _window: std::marker::PhantomData,
        command_buf_pool: Vec::new(),
        frame_text: Vec::new(),
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
    /// Drains and applies the window-management commands a handler enqueued — from a title-bar control during
    /// event dispatch, or from `on_frame` (e.g. raising this window on a routed handoff). Returns whether any
    /// applied. Routed through the App bridge so the dylib-backed `HotApp` drains the dylib's own queue.
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
                WindowCommand::SetCursor(cursor) => window.set_cursor(cursor),
                WindowCommand::Close => self.exit_requested = true,
            }
        }
        applied
    }

    /// Enters this handler's surface world for the duration of a lifecycle call, so its build/event/frame
    /// resolve layout/overlay/focus (and the reactive current-surface) against the right surface. Returns
    /// `None` for a single-window app — its ambient world is its one surface — making this a zero-cost no-op.
    /// The returned guard owns the restore state and does not borrow `self`, so callers can mutate `self`
    /// while it is held (`let _surface = self.enter_surface();`).
    /// Drops the previous tree, builds the app's UI again, and starts it at the surface's real size and past
    /// every generation already drawn.
    ///
    /// The generation step is the load-bearing part, and the reason this is one function: the counter lives on
    /// the tree, so a fresh tree starts over and a surface whose content never changes hands the renderer a
    /// number it has already presented — which it answers by re-presenting the texture it retained. See
    /// [`FrameGeneration`]. Callers must have `scale_factor` and `renderer_transparent`
    /// current before calling.
    fn mount_tree(&mut self, window: &W) {
        // Dropped before the new one is built: an effect from the outgoing tree that re-runs while its
        // replacement is being assembled would write into widgets nothing is drawing any more.
        self.tree = None;
        self.tree = Some(self.app.mount());
        self.generation.restart();
        if self.is_transparent() != self.renderer_transparent {
            self.pending_restart = true;
        }
        // A tree starts at its 0×0 defaults and learns the surface's real size from this event, exactly as it
        // would from a monitor's own resize.
        let resize = Event::WindowResized {
            width: (window.width() as f32 / self.scale_factor) as u32,
            height: (window.height() as f32 / self.scale_factor) as u32,
        };
        if let Some(ref mut tree) = self.tree {
            tree.on_event(&resize);
        }
    }

    fn enter_surface(&self) -> Option<LifecycleGuard> {
        self.surface.as_ref().map(|s| LifecycleGuard {
            _surface: s.enter(),
            _commands: self.window_commands.enter(),
        })
    }

    /// Swaps in a rebuilt dylib, or shows the banner for one that failed to build. `true` means the frame is
    /// the reload's own and this pass is over.
    #[cfg(all(feature = "dev", not(target_os = "android")))]
    fn poll_hot_reload(&mut self, window: &W) -> bool {
        let Some(rx) = &self.hot_reload_rx else {
            return false;
        };
        let Ok(event) = rx.try_recv() else {
            return false;
        };
        match event {
            crate::hot::HotEvent::BuildError(msg) => {
                self.dev.set_build_error(Some(msg));
                window.request_redraw();
                false
            }
            crate::hot::HotEvent::Reload(new_path) => match crate::hot::load_hot_app(&new_path) {
                Ok(new_app) => {
                    // Carry serializable hot state into the incoming dylib while the old tree (and its signals) is still alive; hot_signal consumes it as components remount.
                    if let Some(blob) = self.app.hot_snapshot() {
                        new_app.hot_restore(&blob);
                    }
                    // Drop the old tree first so effect closures (which contain code from the old dylib) are destroyed while the old lib is still mapped. Only then replace self.app, which dlcloses the old dylib.
                    self.tree = None;
                    self.app = Box::new(new_app);
                    self.mount_tree(window);
                    // A successful reload clears any banner from the previous failed build.
                    self.dev.set_build_error(None);
                    tracing::info!("hot reloaded: {}", new_path.display());
                    window.request_redraw();
                    true
                }
                Err(e) => {
                    tracing::error!("hot reload failed: {e}");
                    false
                }
            },
        }
    }

    /// Asks the host to start a renderer. `false` means the surface cannot present at all; a build still in flight
    /// counts as success, because the frame its own wake asks for is what installs it.
    ///
    /// `backend` is passed rather than read from `self` because two callers want something else: the dev toggle
    /// asks for hardware before its saved preference is re-read, and the `Auto` fallback asks for software without
    /// giving up on hardware for the next restart.
    fn start_renderer(&mut self, window: &W, backend: RendererBackend) -> bool {
        let transparent = self.is_transparent();
        let request = RendererRequest {
            backend,
            transparent,
            font_paths: &self.font_paths,
            font_data: &self.font_data,
            font_family: self.font_family.as_deref(),
            paths: self.paths.as_ref(),
            app_name: &self.app_name,
        };
        match self.renderer_host.start(window, &request) {
            RendererStart::Started { keepalive, label } => {
                self.renderer_keepalive = keepalive;
                self.dev.set_renderer_info(label);
                true
            }
            RendererStart::Building => true,
            RendererStart::Failed(e) => {
                tracing::error!("renderer creation failed: {e}");
                false
            }
        }
    }

    /// Installs the renderer a background build finished, or asks for another frame while it is still going.
    ///
    /// The re-ask is not belt-and-braces: a window with no renderer requests no frames, so the wake the
    /// builder sends is the only thing pacing this loop — and it can land a hair before the thread has
    /// actually exited.
    fn poll_pending_renderer(&mut self, window: &W) {
        let Some(outcome) = self.renderer_host.poll() else {
            if self.renderer_host.is_building() {
                window.request_redraw();
            }
            return;
        };
        match outcome {
            RendererStart::Started { keepalive, label } => {
                self.renderer_keepalive = keepalive;
                self.dev.set_renderer_info(label);
            }
            RendererStart::Building => {}
            // Every path that builds a renderer arrives here, so the fallback lives here too: an `Auto` app whose adapter is missing (or lost, on a restart that already dropped the working one) gets software rather than a window that presents nothing. The configured backend stays `Auto`, so a later restart tries hardware again.
            RendererStart::Failed(e) if matches!(self.backend, RendererBackend::Auto) => {
                tracing::warn!("HW renderer unavailable ({e}), falling back to SW");
                self.renderer_host.retire();
                self.start_renderer(window, RendererBackend::Software);
            }
            RendererStart::Failed(e) => {
                tracing::error!("Background HW renderer creation failed: {e}")
            }
        }
        window.request_redraw();
    }

    /// Rebuilds the renderer after a backend switch or a transparency change.
    fn apply_pending_restart(&mut self, window: &W) {
        if !self.pending_restart {
            return;
        }
        self.pending_restart = false;
        self.renderer_transparent = self.is_transparent();
        self.backend = self
            .prefs
            .backend
            .unwrap_or_else(config::compile_time_backend);
        // Drop the old renderer and its thread before creating the new one, to avoid peak memory overlap. Nothing is kept warm: a restart exists to change what a renderer is baked with.
        drop(self.renderer.take());
        self.renderer_host.retire();
        // Nothing presents until a hardware build lands; the poll site takes the `Auto` fallback if it fails.
        self.start_renderer(window, self.backend);
    }

    /// Whether an idle blit is worth submitting.
    ///
    /// Keepalive is a power policy of the renderer that is running, not a property of having a render thread:
    /// hardware keeps taking frames while idle so the blit holds the device in an active power state, whereas
    /// re-rasterising an unchanged frame on the CPU buys nothing. Every backend has a render thread, so the
    /// host reports which kind it started rather than it being inferred here.
    ///
    /// What it is worth paying for is decided by **focus**, and that is the whole point: holding the GPU awake
    /// is insurance against the next frame arriving late, and a frame only arrives from somebody who is there.
    /// A window nobody is looking at will not be typed into, so it can sleep at once; a focused one is one key
    /// press away from needing the GPU, however long it has been still. [`IDLE_GRACE`] is only the backstop
    /// for the window left focused and abandoned.
    fn keepalive_due(&self) -> bool {
        if self.dev.keepalive_interval().is_some() {
            return true;
        }
        self.renderer_keepalive && self.focused && self.last_input.elapsed() < IDLE_GRACE
    }

    /// Whether to submit a frame now, and whether it carries new content. The second of the pass's three
    /// clocks: the frame budget gates the whole pass, this gates submission, `about_to_wait` reports the next
    /// wake. `None` means skip this turn.
    fn frame_is_due(&self, now: std::time::Instant) -> Option<bool> {
        // A continuous region carries new content the tree cannot report: the application repaints its own texture and the draw commands naming it never change. Counting only `is_dirty` here drops such a frame through to the 1 fps keepalive below, which shows a region refilled at sixty once a second.
        let has_content = self.tree.as_ref().map(|t| t.is_dirty()).unwrap_or(false)
            || self.app.motion_has_continuous();
        let needs_keepalive = self.keepalive_due();
        if !has_content && !needs_keepalive {
            return None;
        }
        // A keepalive blit carries no new content, so it runs at the keepalive cadence rather than the frame rate. Enforced here instead of left to `about_to_wait`'s reported interval because a submitted frame is itself a wakeup: its commit makes the compositor fd readable, returning the next dispatch immediately to compose another one.
        let keepalive_interval = self
            .dev
            .keepalive_interval()
            .unwrap_or(HW_KEEPALIVE_INTERVAL);
        if !has_content && now.duration_since(self.last_submit) < keepalive_interval {
            return None;
        }
        Some(has_content)
    }

    /// The generation this frame's commands go out under: the tree's own, offset past every tree this surface
    /// has had before it. See [`FrameGeneration`] for why the offset has to exist.
    ///
    /// **Compose first.** The tree's counter is bumped by `commands()`, so a generation read before this
    /// frame is composed carries the *previous* frame's number while the commands beside it carry this
    /// frame's content — and a renderer whose whole contract is "equal generations mean identical commands"
    /// then re-presents the frame before this one. On a screen with an animation it hides, because the
    /// continuous counter moves anyway; on a still screen it costs the change until something else redraws.
    fn frame_generation(&mut self) -> u64 {
        let composed = self
            .tree
            .as_ref()
            .map(|t| {
                drop(t.frame());
                t.generation()
            })
            .unwrap_or(0);
        self.generation
            .next(composed, self.app.motion_has_continuous())
    }

    /// Whether the app asked for a transparent surface (`WindowConfig::is_transparent`). Read at each renderer creation so hardware picks a premultiplied-alpha composite mode and software presents an alpha-preserving buffer.
    fn is_transparent(&self) -> bool {
        self.app
            .window_config()
            .map(|c| c.is_transparent)
            .unwrap_or(false)
    }

    /// Rasterises a frame inline, for the offscreen renderer that has no thread of its own.
    ///
    /// Headless runs, `[preview]` captures and `cargo telar test` all read the pixels back with
    /// `last_frame_rgba` in the same call that asked for them, so this one has to stay synchronous — and it
    /// deliberately does *not* wrap the render in `catch_unwind`, because a panic here is a test failure to
    /// surface rather than a dropped frame to recover from.
    fn render_offscreen(&mut self, msg: FrameMsg) {
        let Some(renderer) = &mut self.renderer else {
            return;
        };
        if let Err(e) =
            renderer.begin_frame(msg.width, msg.height, msg.scale_factor, msg.generation)
        {
            tracing::error!("begin_frame failed: {e}");
            return;
        }
        let gpu_start = renderer_core::perf::now_if_enabled();
        let commands: &[renderer_core::DrawCommand] =
            if renderer.applies_scale_factor() || msg.scale_factor == 1.0 {
                &msg.commands
            } else {
                self.scale_scratch
                    .scale_into(&msg.commands, msg.scale_factor)
            };
        if let Err(e) = renderer.as_mut().render_frame(commands, msg.clear) {
            tracing::error!("render_frame failed: {e}");
        }
        renderer_core::perf::record_since(renderer_core::perf::Phase::Gpu, gpu_start);
        if self.command_buf_pool.len() < COMMAND_BUF_POOL_CAP {
            self.command_buf_pool.push(msg.commands);
        }
    }
}

impl<W, D> EventHandler<W> for AppHandler<W, D>
where
    W: Window + Clone + Send + Sync + 'static,
    D: DevPlugin,
{
    fn accessibility(&self) -> Vec<platform_core::AccessNode> {
        let _surface = self.enter_surface();
        ui_core::accessibility::snapshot(&self.frame_text)
    }

    fn on_accessibility_action(&mut self, id: u64, activate: bool) {
        let _surface = self.enter_surface();
        ui_core::focus::request(id);
        // Activation goes out as the key a focused control already answers, rather than as a third path into
        // the same commit. A reader pressing a button then travels the route the keyboard travels — the one
        // that has tests, and that stays right because everyone uses it.
        if activate {
            let enter = platform_core::Event::KeyPressed {
                key: platform_core::Key::Named(platform_core::NamedKey::Enter),
                modifiers: platform_core::ModifiersState::default(),
            };
            if let Some(tree) = &mut self.tree {
                tree.on_event(&enter);
            }
        }
    }

    fn on_resume(&mut self, window: &W) -> bool {
        let _surface = self.enter_surface();
        // Load the app's faces before the tree below measures a word of text. Building a renderer loads them too — that is what makes measure and draw agree — but a hardware renderer builds on its own thread, so waiting for it would leave the first layout sized in the platform's fonts. The renderer's own load then finds these already there and clones the database instead of scanning again.
        //
        // A renderer that does not shape glyphs — a terminal — skips the scan outright: it opens no font file,
        // and the measurer it installed for itself must not be replaced by the raster one this would install.
        if self.renderer_host.shapes_text() {
            let system_fonts = SystemFonts::from_provider(self.paths.as_ref());
            renderer_text::fonts::install(build_font_config(
                self.font_paths.clone(),
                self.font_data.clone(),
                self.font_family.clone(),
                &system_fonts,
            ));
        }
        // Offscreen/headless windows have no surface, so a windowed renderer can't create one: rasterize into a
        // CPU pixmap (read back via `last_frame_rgba`), forced regardless of the configured backend so the
        // headless path needs no GPU adapter. On-screen windows build the configured renderer.
        let renderer_ok = if window.is_offscreen() {
            let transparent = self.is_transparent();
            let request = RendererRequest {
                backend: self.backend,
                transparent,
                font_paths: &self.font_paths,
                font_data: &self.font_data,
                font_family: self.font_family.as_deref(),
                paths: self.paths.as_ref(),
                app_name: &self.app_name,
            };
            self.renderer = self.renderer_host.build_offscreen(window, &request);
            self.renderer.is_some()
        } else {
            self.start_renderer(window, self.backend)
        };
        if !renderer_ok {
            return false;
        }
        self.renderer_transparent = self.is_transparent();
        let sf = window.scale_factor() as f32;
        self.scale_factor = sf;
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
        // Hand the same wake to the app's reactive runtime so `spawn_task` needs no waker ceremony from the
        // app. Under the per-window fallback above this points at whichever surface resumed last, which is
        // enough: any redraw runs a frame, and every surface's frame drains the whole task queue.
        if let Some(waker) = self.redraw_waker.clone() {
            self.app.install_task_waker(waker);
        }
        #[cfg(all(feature = "dev", not(target_os = "android")))]
        if let Some(rx) = self.hot_reload_rx.take() {
            let (relay_tx, relay_rx) = std::sync::mpsc::channel::<crate::hot::HotEvent>();
            let window_clone = window.clone();
            std::thread::Builder::new()
                .name("telar-hot-relay".to_string())
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
        self.mount_tree(window);

        let w = window.clone();
        self._flush_notify = Some(set_flush_notify(move || w.request_redraw()));
        window.request_redraw();
        true
    }

    /// Builds the app's UI again on the surface it is already running on, dropping the previous tree first so
    /// its effects and their subscriptions go with it.
    ///
    /// The window, the renderer and the surface's place on screen are untouched — this is a re-render, not a
    /// restart. The one exception is transparency, which a renderer is *built* with: an app that now asks for
    /// the other kind gets its renderer rebuilt on the next frame, which is the same path a backend switch
    /// takes.
    fn remount(&mut self, window: &W) {
        let _surface = self.enter_surface();
        self.mount_tree(window);
        window.request_redraw();
    }

    fn on_event(&mut self, event: Event, window: &W) {
        let _surface = self.enter_surface();
        // Before dispatch, so a handler running on this very event already sees the state it establishes: a `Shift`-click's press handler has to read the modifiers the click arrived under, and a drag handler has to read the button that started it.
        ui_core::observe_keyboard(&event);
        ui_core::observe_pointer(&event);
        // Who the keepalive is for, kept here rather than in the match below because it reads across events
        // the runner otherwise lets straight through.
        if let Event::FocusChanged { is_focused } = &event {
            self.focused = *is_focused;
        }
        if matches!(
            event,
            Event::KeyPressed { .. }
                | Event::KeyReleased { .. }
                | Event::PointerMoved { .. }
                | Event::PointerPressed { .. }
                | Event::PointerReleased { .. }
                | Event::Scrolled { .. }
        ) {
            self.last_input = std::time::Instant::now();
        }
        // The four events the runner reads before the app does, matched once. Written as four sequential
        // `if let`s it re-tested the same value each time, and the two that end the dispatch read as though
        // they were guards on the ones above them rather than exits.
        match &event {
            Event::ScaleFactorChanged { scale_factor } => self.scale_factor = *scale_factor as f32,
            Event::ColorSchemeChanged { dark } => {
                // Drives the follow_system effect (which writes the theme signal); batch the app's runtime so the
                // re-render flushes cleanly across the hot-reload boundary. No widget consumes this event.
                self.app.begin_event_batch();
                self.app.set_system_dark(*dark);
                self.app.end_event_batch();
                window.request_redraw();
                return;
            }
            Event::KeyPressed { key, modifiers } => match self.dev.on_key(key, *modifiers) {
                DevAction::Redraw => window.request_redraw(),
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
                        RendererBackend::Software => self.pending_restart = true,
                        // Straight to a background build rather than through `pending_restart`, so the running renderer keeps presenting until the new one lands.
                        _ => {
                            self.start_renderer(window, RendererBackend::Hardware);
                        }
                    }
                }
                DevAction::None => {}
            },
            Event::PointerPressed { x, y, .. } => {
                if self.dev.on_pointer_pressed(*x as f32, *y as f32) {
                    window.request_redraw();
                    return;
                }
            }
            _ => {}
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
            // Flush reactive effects immediately so on_redraw() in the same cycle finds tree_dirty=true rather than deferring to the next cycle.
            end_batch();
            begin_batch();
            window.request_redraw();
        }
    }

    fn on_redraw(&mut self, window: &W) {
        let _surface = self.enter_surface();
        #[cfg(all(feature = "dev", not(target_os = "android")))]
        if self.poll_hot_reload(window) {
            return;
        }
        // Drive the motion engine before tree_dirty is read below: tick()'s .set() calls only enqueue effects while a batch is open (new_events already opened one), so force a flush here to re-run any segment reading an animated value now, not on the next cycle. This is what makes an animation-only frame (no user event, tree otherwise clean) observe interpolated values in this same frame's tree.commands(). The tree is mounted in the app's own runtime (`crate::tree`), so those values reach the segments that read them even under hot reload — a host-mounted tree would instead re-send the commands composed for the animation's first value (a page stuck at opacity 0) until some event forced a re-render.
        // Everything below composes a frame, so pace the whole pass rather than only capping the render at the end. A platform may call `on_redraw` on every loop turn, and the pass then schedules its own next call — the motion tick's `.set()` writes flush, and the flush notifies the platform to redraw — so the loop free-runs instead of sleeping.
        let now = std::time::Instant::now();
        if now.duration_since(self.last_tick) < FRAME_BUDGET {
            return;
        }
        self.last_tick = now;
        // Deliver finished background work before the tick: the batch open here defers its flush to the
        // `end_batch` below, so a task that dirties layout is picked up by the `relayout` that follows
        // instead of waiting a frame.
        self.app.drain_tasks();
        self.app.motion_tick(now);
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
            // `None` when this surface has no OS handles to give: the entry point that built the handler decided.
            let (raw_window_handle, raw_display_handle) = self
                .raw_handles
                .map(|of| of(window))
                .unwrap_or((None, None));
            let mut ctx = crate::app_context::AppCtx {
                redraw_requested: &mut redraw_requested,
                redraw_waker: self.redraw_waker.as_ref(),
                raw_window_handle,
                raw_display_handle,
            };
            self.app.on_frame(&mut ctx);
        }
        // After `on_frame`, which is where an app reads them: a press has to answer true for the whole frame it arrived in, and stop answering in the next.
        ui_core::end_keyboard_frame();
        // And again on the tree's own side, which is a different set of registries whenever the tree lives
        // behind a dylib boundary; a no-op for a tree in this process.
        if let Some(ref tree) = self.tree {
            tree.end_frame();
        }
        // Apply commands enqueued during on_frame (e.g. raising this window on a routed handoff): on_event's
        // drain only runs on input events, so a frame-driven command would otherwise wait for the next one.
        self.apply_window_commands(window);
        if redraw_requested {
            window.request_redraw();
        }

        self.poll_pending_renderer(window);
        self.apply_pending_restart(window);

        let Some(has_content) = self.frame_is_due(now) else {
            return;
        };
        self.last_submit = now;
        // Only update last_frame for content frames; keepalive blits must not reset the budget clock (would delay next content render by up to 16ms).
        if has_content {
            self.last_frame = now;
        }

        let (w, h) = (window.width(), window.height());
        tracing::debug!(
            "on_redraw: window {}x{} scale={} has_content={}",
            w,
            h,
            self.scale_factor,
            has_content
        );

        // Flush reactive effects so clear_color and draw commands come from the same reactive pass. Without
        // this, a RedrawRequested that fires before about_to_wait (e.g. a keepalive blit) reads clear_color
        // from the new signal value while commands still reflect the previous view() call.
        end_batch();
        begin_batch();
        // After that flush, and not before: the number has to describe the commands this frame ships, and the
        // effects that decide them have only just run.
        let generation = self.frame_generation();
        renderer_core::perf::tick();
        // F2: reclaim buffers the render thread finished with, capped so the free-list stays tiny.
        if let Some(channels) = self.renderer_host.channels() {
            while let Ok(buf) = channels.ret_rx.try_recv() {
                if self.command_buf_pool.len() < COMMAND_BUF_POOL_CAP {
                    self.command_buf_pool.push(buf);
                }
            }
        }
        let build_start = renderer_core::perf::now_if_enabled();
        let clear = self.app.clear_color();
        let commands_ref = self.tree.as_ref().map(|t| t.frame());
        let base_slice: &[renderer_core::DrawCommand] = commands_ref.as_deref().unwrap_or(&[]);
        if let Some(tree) = &self.tree {
            let mut nodes = Vec::new();
            tree.walk(&mut nodes);
            self.dev.on_tree(&nodes);
        }
        let logical_w = w as f32 / self.scale_factor;
        let logical_h = h as f32 / self.scale_factor;
        let frame_commands = self
            .dev
            .on_frame(base_slice, logical_w, logical_h, has_content);
        renderer_core::perf::record_since(renderer_core::perf::Phase::Build, build_start);
        let clone_start = renderer_core::perf::now_if_enabled();
        // F2: refill a recycled buffer instead of allocating a fresh Vec every frame.
        let mut commands = self.command_buf_pool.pop().unwrap_or_default();
        commands.clear();
        commands.extend_from_slice(&frame_commands);
        renderer_core::perf::record_since(renderer_core::perf::Phase::Clone, clone_start);
        // Kept before the frame is handed off, because naming a control means asking what text was drawn
        // inside it, and by the time anything asks the frame belongs to the render thread.
        self.frame_text.clear();
        self.frame_text.extend(
            frame_commands
                .iter()
                .filter(|c| matches!(c, renderer_core::DrawCommand::Text { .. }))
                .cloned(),
        );
        // The message owns its commands from here on; release the borrows of the tree and the dev plugin so
        // the offscreen branch below can take `&mut self`.
        drop(frame_commands);
        drop(commands_ref);
        let msg = FrameMsg {
            width: w,
            height: h,
            scale_factor: self.scale_factor,
            generation,
            commands,
            clear,
            timestamp: std::time::Instant::now(),
        };

        // On-screen: hand the frame to the render thread and return. Drop it if that thread is still busy,
        // which is what keeps input handling off the rasteriser's critical path. On a dropped or disconnected
        // send, recover the buffer for the free-list instead of freeing it.
        if let Some(channels) = self.renderer_host.channels() {
            if let Err(e) = channels.tx.try_send(msg) {
                let recovered = match e {
                    std::sync::mpsc::TrySendError::Full(m)
                    | std::sync::mpsc::TrySendError::Disconnected(m) => m.commands,
                };
                if self.command_buf_pool.len() < COMMAND_BUF_POOL_CAP {
                    self.command_buf_pool.push(recovered);
                }
            }
            return;
        }

        self.render_offscreen(msg);
    }

    fn on_suspend(&mut self) {
        let _surface = self.enter_surface();
        // Let the host keep whatever makes the next resume cheap: for hardware, a device to rebind rather than rebuild.
        self.renderer_host.suspend();
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
        if tree_dirty || self.app.motion_has_active() || self.app.motion_has_continuous() {
            // Against `last_tick`, the clock `on_redraw` actually gates on: reporting a deadline the pass would decline to act on wakes the loop early and it spins re-asking. The two disagree whenever a tick leaves the tree clean, advancing `last_tick` but not `last_frame`.
            Some(FRAME_BUDGET.saturating_sub(self.last_tick.elapsed()))
        } else {
            if let Some(interval) = self.dev.keepalive_interval() {
                // Dev plugin drives its own cadence (e.g. FPS counter tick-down).
                Some(interval)
            } else if self.keepalive_due() {
                Some(HW_KEEPALIVE_INTERVAL)
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

    // Entering the surface is the whole point: the registry is one of its per-surface worlds, so a caller outside this handler reads the ambient (always empty) one instead.
    fn interactive_rects(&self) -> Vec<geometry_core::Rect> {
        let _surface = self.enter_surface();
        ui_core::interactive_rects()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use platform_headless::HeadlessWindow;

    /// An app whose content never changes — a shell's frame ring, a wallpaper, a static diagram. Its tree's
    /// own generation is fixed for the life of the tree, which is what makes the collision below reachable.
    struct Unchanging;

    impl App for Unchanging {
        fn root(&self) -> Box<dyn ui_tree::Component> {
            ui_core::reset_layout_runtime();
            Box::new(
                ui_core::Rectangle::new(
                    layout_core::LayoutStyle::new().width(10.0).height(10.0),
                    || renderer_core::RectStyle::filled(renderer_core::Color::BLACK, 0.0),
                )
                .expect("a rectangle builds"),
            )
        }
    }

    fn handler() -> AppHandler<HeadlessWindow, ()> {
        build_app_handler::<HeadlessWindow, ()>(
            Box::new(Unchanging),
            Arc::new(services_core::NoPaths),
            Vec::new(),
            Vec::new(),
            None,
            RendererBackend::Software,
            UserPrefs::default(),
            "generation-test".to_string(),
            SurfaceRenderer::builtin(),
        )
    }

    /// The keepalive exists so the next frame does not wait for the GPU to clock back up, and only somebody
    /// who is there can ask for a next frame. What it used to key on was a timer since the last *frame*, which
    /// measures the wrong thing: a screen being read produces no frames at all, so it slept three seconds into
    /// somebody looking straight at it.
    #[test]
    fn the_gpu_is_held_awake_for_whoever_is_there_to_notice() {
        let mut handler = handler();
        handler.renderer_keepalive = true;

        assert!(
            handler.keepalive_due(),
            "a focused window keeps the GPU warm however long it has been still"
        );

        handler.focused = false;
        assert!(
            !handler.keepalive_due(),
            "a window nobody is looking at will not be typed into"
        );

        handler.focused = true;
        handler.last_input = std::time::Instant::now() - IDLE_GRACE - FRAME_BUDGET;
        assert!(
            !handler.keepalive_due(),
            "a window left focused and abandoned sleeps on the backstop"
        );
    }

    /// A software renderer has no GPU to hold awake, so re-rasterising an unchanged frame buys nothing.
    #[test]
    fn the_rasteriser_never_asks_for_an_idle_frame() {
        let mut handler = handler();
        handler.renderer_keepalive = false;
        assert!(!handler.keepalive_due());
    }

    /// An app whose one rectangle takes its colour from a signal: the smallest thing that can change without
    /// anything animating, which is the case the generation got wrong.
    struct Tinted(reactive_core::RwSignal<f32>);

    impl App for Tinted {
        fn root(&self) -> Box<dyn ui_tree::Component> {
            ui_core::reset_layout_runtime();
            let tint = self.0;
            Box::new(
                ui_core::Rectangle::new(
                    layout_core::LayoutStyle::new().width(10.0).height(10.0),
                    move || {
                        renderer_core::RectStyle::filled(
                            renderer_core::Color::rgba(tint.get(), 0.0, 0.0, 1.0),
                            0.0,
                        )
                    },
                )
                .expect("a rectangle builds"),
            )
        }
    }

    /// A tree that changed must not ship its new commands under the old number.
    ///
    /// The renderer's whole contract is that equal generations mean identical commands, and it acts on it: the
    /// hardware backend skips its pipeline and re-presents the frame it retained. Reading the counter before
    /// composing broke it in the one case nothing else covers — a still screen, where no animation is moving
    /// the number anyway. On screen it looked like a menu answering a key press a second late, because the
    /// correct frame was discarded and the next keepalive blit was what finally drew it.
    #[test]
    fn new_commands_never_go_out_under_the_previous_generation() {
        let tint = reactive_core::signal(0.0f32);
        let mut handler = build_app_handler::<HeadlessWindow, ()>(
            Box::new(Tinted(tint)),
            Arc::new(services_core::NoPaths),
            Vec::new(),
            Vec::new(),
            None,
            RendererBackend::Software,
            UserPrefs::default(),
            "generation-test".to_string(),
            SurfaceRenderer::builtin(),
        );
        handler.tree = Some(handler.app.mount());

        let first = handler.frame_generation();
        assert_eq!(
            handler.frame_generation(),
            first,
            "a frame nothing changed for must keep the number, or the renderer redraws for nothing"
        );

        tint.set(0.5);
        assert_ne!(
            handler.frame_generation(),
            first,
            "the changed frame reused the previous number, so the renderer re-presents the old one"
        );
    }

    /// A rebuilt tree must never hand the renderer a generation it has already drawn — see
    /// [`FrameGeneration`] for why one otherwise would.
    ///
    /// What it looked like: a shell's config reload moved the space its bars reserved but left the frame ring
    /// and the wallpaper exactly as they were, until the process was restarted. The bars followed the edit,
    /// because a ticking clock had already carried their counter past the collision.
    #[test]
    fn a_remounted_tree_never_reuses_a_generation_the_renderer_has_drawn() {
        let mut handler = handler();
        let window = HeadlessWindow::new(120, 80);

        handler.tree = Some(handler.app.mount());
        let first = handler.frame_generation();

        // The collision, stated: the fresh tree's own counter is back where the outgoing one started.
        handler.remount(&window);
        let composed = handler.tree.as_ref().map(|t| t.generation()).unwrap_or(0);
        let second = handler.frame_generation();

        assert!(
            second > first,
            "a rebuilt tree reported generation {second} after {first} was already drawn, so the renderer \
             would blit the frame from before the rebuild"
        );
        assert!(
            composed <= first,
            "this test proves nothing unless the new tree's own counter really is back in drawn territory: \
             it reported {composed} against {first}"
        );

        // A tree that is *not* rebuilt keeps its generation, which is what lets the renderer skip idle frames.
        assert_eq!(
            handler.frame_generation(),
            second,
            "an unchanged tree must keep reporting the same generation, or every idle frame re-renders"
        );
    }

    /// A region filled from outside must not be caught by the idle-frame fast path.
    ///
    /// The trap is that everything looks right: the application renders into its texture at its own pace and
    /// Telar schedules frames for it, but the draw commands pointing at that texture are identical every time
    /// — the id addresses the view, not its contents, deliberately. Equal generations then tell the renderer
    /// it may re-present what it retained, and the window shows one frozen frame while the application keeps
    /// repainting behind it at full speed.
    #[test]
    fn a_continuous_region_moves_the_generation_though_its_commands_never_change() {
        let mut handler = handler();
        let window = HeadlessWindow::new(120, 80);
        handler.tree = Some(handler.app.mount());

        let at_rest = handler.frame_generation();
        assert_eq!(
            handler.frame_generation(),
            at_rest,
            "this app's commands are fixed, so without a region nothing should move"
        );

        let region = motion_core::Continuous::new();
        let first = handler.frame_generation();
        let second = handler.frame_generation();
        assert!(
            first > at_rest && second > first,
            "the generation stalled at {at_rest}/{first}/{second}, so the renderer would blit a stale frame"
        );

        drop(region);
        let after = handler.frame_generation();
        assert_eq!(
            handler.frame_generation(),
            after,
            "with the region gone the surface must go back to skipping idle frames"
        );
        let _ = window;
    }

    /// A continuous region has to survive **three** gates, and the third is the one that bites hardest.
    ///
    /// `about_to_wait` must keep scheduling frames, `frame_generation` must keep moving so the renderer
    /// cannot re-present what it retained — and this one: a frame whose tree is clean falls through to the
    /// keepalive branch, which runs at **1 fps**. A region that cleared the first two and not this one is
    /// composed once a second while the application refills it at sixty, which reads as a renderer that is
    /// merely slow.
    #[test]
    fn a_clean_tree_with_a_continuous_region_is_still_worth_a_frame() {
        let mut handler = handler();
        let window = HeadlessWindow::new(120, 80);
        assert!(handler.on_resume(&window), "a headless resume builds one");
        // The platform opens a batch before dispatching, and `on_redraw` closes and reopens it; without one already open it closes a batch that was never begun.
        begin_batch();

        // Forced open before each pass: this is about what counts as content, not about the frame clock.
        let opened = || std::time::Instant::now() - FRAME_BUDGET * 2;

        handler.last_tick = opened();
        handler.on_redraw(&window);
        let first = handler.last_submit;

        handler.last_tick = opened();
        handler.on_redraw(&window);
        assert_eq!(
            handler.last_submit, first,
            "a clean tree with nothing else to say must not submit a frame"
        );

        let _awake = motion_core::Continuous::new();
        handler.last_tick = opened();
        handler.on_redraw(&window);
        assert!(
            handler.last_submit > first,
            "the region says the picture changed even though the tree cannot, so this frame had to go out"
        );
    }

    /// The keyboard registry is wired into the runner, not just into `ui-core`.
    ///
    /// Its own unit tests drive `observe` directly, so they would pass just as well with the runner never
    /// calling it — and a modifier state nobody feeds is worse than none, because it answers confidently
    /// with whatever it last saw.
    #[test]
    fn the_runner_feeds_the_keyboard_registry() {
        ui_core::reset_keyboard();
        let mut handler = handler();
        let window = HeadlessWindow::new(120, 80);

        assert!(!ui_core::modifiers().is_shift);
        handler.on_event(
            Event::ModifiersChanged {
                modifiers: platform_core::ModifiersState {
                    is_shift: true,
                    ..Default::default()
                },
            },
            &window,
        );
        assert!(
            ui_core::modifiers().is_shift,
            "a bare Shift must reach the registry, since it maps to no Key at all"
        );

        handler.on_event(
            Event::KeyPressed {
                key: platform_core::Key::Named(platform_core::NamedKey::ArrowUp),
                modifiers: Default::default(),
            },
            &window,
        );
        let up = platform_core::Key::Named(platform_core::NamedKey::ArrowUp);
        assert!(ui_core::key_held(&up));
        assert!(ui_core::key_pressed(&up));
    }

    /// The same for the pointer's buttons, and for the same reason: a drag handler asks which button is
    /// doing the dragging, and a registry nobody feeds answers confidently with nothing.
    #[test]
    fn the_runner_feeds_the_pointer_button_registry() {
        ui_core::reset_pointer();
        let mut handler = handler();
        let window = HeadlessWindow::new(120, 80);

        assert!(!ui_core::pointer_buttons().any());
        handler.on_event(
            Event::PointerPressed {
                x: 10.0,
                y: 10.0,
                button: platform_core::PointerButton::Secondary,
                source: platform_core::PointerSource::Mouse,
            },
            &window,
        );
        assert!(ui_core::pointer_buttons().secondary);
        handler.on_event(
            Event::PointerReleased {
                x: 10.0,
                y: 10.0,
                button: platform_core::PointerButton::Secondary,
                source: platform_core::PointerSource::Mouse,
            },
            &window,
        );
        assert!(!ui_core::pointer_buttons().any());
    }
}
