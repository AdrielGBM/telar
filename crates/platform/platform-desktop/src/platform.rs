use std::collections::HashMap;
use std::sync::Arc;

use platform_core::{
    Event, EventHandler, FullscreenMode, MultiSurfacePlatform, Platform, PlatformError, SurfaceId,
    Window, WindowConfig, WindowPosition,
};
use winit::application::ApplicationHandler;
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Fullscreen, WindowAttributes, WindowId, WindowLevel};

use platform_winit::{SurfaceIntent, WinitWindow, map_window_event};

// Winit user-event payloads injected from background threads (via EventLoopProxy) to wake the loop.
enum UserEvent {
    // A screen reader asked for the tree, asked to act on a node, or went away. Routed through the loop
    // rather than answered where it arrives: AccessKit calls its handlers on a platform thread, and the UI
    // that has to answer is `!Send` — it lives on this one.
    Accessibility(accesskit_winit::Event),
    // The OS color-scheme flipped; carries the new dark (`true`) / light preference. Linux only: it is the
    // portal watch that sends it, where winit reports nothing. Elsewhere winit delivers `ThemeChanged` itself.
    #[cfg(target_os = "linux")]
    ColorScheme(bool),
    // A background thread asked to wake the UI (via the app redraw waker); redraw every live surface so each
    // one's `on_frame` runs and drains its channels — wherever the waking app's content currently lives.
    Wake,
}

impl From<accesskit_winit::Event> for UserEvent {
    fn from(event: accesskit_winit::Event) -> Self {
        UserEvent::Accessibility(event)
    }
}

pub struct WinitPlatform {
    event_loop: EventLoop<UserEvent>,
}

impl WinitPlatform {
    pub fn try_new() -> Result<Self, PlatformError> {
        Ok(Self {
            event_loop: EventLoop::<UserEvent>::with_user_event()
                .build()
                .map_err(|e| PlatformError(e.to_string()))?,
        })
    }
}

struct WinitRunner<H: EventHandler<WinitWindow>> {
    handler: H,
    window: Option<WinitWindow>,
    config: WindowConfig,
    cursor_position: (f64, f64),
    scale_factor: f64,
    modifiers: platform_core::ModifiersState,
    // True only on WaitUntil timer expiry; gates keepalive request_redraw() so it doesn't fire on every event queue drain.
    timer_has_fired: bool,
    // Built at resume, before the window is shown, which the adapter requires. The tree behind it is only ever
    // assembled while an assistive technology is attached — which `update_if_active` is the whole point of.
    a11y: Option<accesskit_winit::Adapter>,
    a11y_proxy: winit::event_loop::EventLoopProxy<UserEvent>,
    // The nodes last published, so a request naming one can be mapped back to the control it means.
    a11y_nodes: Vec<platform_core::AccessNode>,
}

impl<H: EventHandler<WinitWindow>> WinitRunner<H> {
    /// Answers a screen reader, on the thread the UI actually lives on.
    fn on_accessibility(&mut self, event: accesskit_winit::WindowEvent) {
        use accesskit_winit::WindowEvent as AkEvent;
        match event {
            AkEvent::InitialTreeRequested => self.publish_accessibility(),
            AkEvent::ActionRequested(request) => {
                let Some((id, activate)) =
                    crate::accessibility::requested_focus_id(&request, &self.a11y_nodes)
                else {
                    return;
                };
                // Through the same door the keyboard uses. A reader activating a button takes the path a press
                // takes, or the two grow apart and it is always the untested one that rots.
                self.handler.on_accessibility_action(id, activate);
                self.publish_accessibility();
            }
            // Nothing to tear down: the tree is only ever built on demand, so ceasing to be asked is the whole
            // of stopping.
            AkEvent::AccessibilityDeactivated => self.a11y_nodes.clear(),
        }
    }

    /// Hands the current tree over, and only if something is listening — which is what keeps the cost of all
    /// this at nothing for the overwhelmingly common case of nobody being.
    fn publish_accessibility(&mut self) {
        let Some(adapter) = &mut self.a11y else {
            return;
        };
        let nodes = self.handler.accessibility();
        let title = self.config.title.clone();
        adapter.update_if_active(|| crate::accessibility::tree_update(&nodes, &title));
        self.a11y_nodes = nodes;
    }
}

impl<H: EventHandler<WinitWindow>> ApplicationHandler<UserEvent> for WinitRunner<H> {
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Accessibility(event) => self.on_accessibility(event.window_event),
            #[cfg(target_os = "linux")]
            UserEvent::ColorScheme(dark) => {
                if let Some(window) = &self.window {
                    self.handler
                        .on_event(Event::ColorSchemeChanged { dark }, window);
                }
            }
            UserEvent::Wake => {
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
        }
    }

    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        self.timer_has_fired = matches!(cause, StartCause::ResumeTimeReached { .. });
        self.handler.new_events();
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(d) = self.handler.about_to_wait() {
            // Only request_redraw() on timer expiry, not every drain; reactive changes call it themselves via flush_notify.
            if self.timer_has_fired {
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(std::time::Instant::now() + d));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let Some(window) = create_window_from_config(event_loop, &self.config) else {
            return;
        };
        // Deliver the OS light/dark preference before the tree mounts, so its first layout uses the
        // right theme. winit reports it on Windows/macOS; on Linux fall back to the freedesktop portal.
        if let Some(dark) = initial_prefers_dark(&window) {
            self.handler
                .on_event(Event::ColorSchemeChanged { dark }, &window);
        }
        if !self.handler.on_resume(&window) {
            event_loop.exit();
            return;
        }
        // Attached before the window is ever visible, which the adapter requires — hence the window being
        // created hidden. The proxy is what routes a reader's requests back onto this thread.
        self.a11y = Some(accesskit_winit::Adapter::with_event_loop_proxy(
            event_loop,
            &window.0,
            self.a11y_proxy.clone(),
        ));
        window.0.set_visible(true);
        window.request_redraw();
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        // Clone (a cheap Arc bump) so the immutable window borrow doesn't conflict with the mutable
        // self.handler/cursor/scale/modifiers borrows the shared dispatcher takes.
        let Some(window) = self.window.clone() else {
            return;
        };
        // The adapter tracks the window itself — focus, size, scale — so it sees the event before anything
        // consumes it.
        if let Some(adapter) = &mut self.a11y {
            adapter.process_event(&window.0, &event);
        }
        let redrawn = matches!(event, WindowEvent::RedrawRequested);
        let outcome = dispatch_window_event(
            &mut self.handler,
            &window,
            &mut self.cursor_position,
            &mut self.scale_factor,
            &mut self.modifiers,
            event,
        );
        // After the frame rather than before: what is announced is then the frame that was drawn.
        if redrawn {
            self.publish_accessibility();
        }
        // A custom title-bar close button sets the handler's exit request during dispatch; honor it alongside
        // the OS close (window manager X / Alt-F4).
        if matches!(outcome, WindowEventOutcome::CloseRequested) || self.handler.take_exit_request()
        {
            event_loop.exit();
        }
    }
}

enum WindowEventOutcome {
    Continue,
    CloseRequested,
}

// Builds a winit window from a `WindowConfig`. Shared by the single- and multi-surface runners.
fn create_window_from_config(
    event_loop: &ActiveEventLoop,
    config: &WindowConfig,
) -> Option<WinitWindow> {
    let mut attributes = WindowAttributes::default()
        // Shown once the accessibility adapter is attached, which has to happen before the window is ever
        // visible. Creating it hidden and revealing it a moment later is what that costs.
        .with_visible(false)
        .with_title(config.title.as_str())
        .with_inner_size(winit::dpi::LogicalSize::new(config.width, config.height))
        .with_resizable(config.is_resizable)
        .with_decorations(config.has_decorations)
        .with_transparent(config.is_transparent);

    if let Some((w, h)) = config.min_size {
        attributes = attributes.with_min_inner_size(winit::dpi::LogicalSize::new(w, h));
    }
    if let Some((w, h)) = config.max_size {
        attributes = attributes.with_max_inner_size(winit::dpi::LogicalSize::new(w, h));
    }
    match config.fullscreen {
        FullscreenMode::Disabled => {}
        // Exclusive requires a concrete video mode; fall back to borderless for now.
        FullscreenMode::Borderless | FullscreenMode::Exclusive => {
            attributes = attributes.with_fullscreen(Some(Fullscreen::Borderless(None)));
        }
    }
    if let WindowPosition::At(x, y) = config.position {
        attributes = attributes.with_position(winit::dpi::PhysicalPosition::new(x, y));
    }
    if config.is_always_on_top {
        attributes = attributes.with_window_level(WindowLevel::AlwaysOnTop);
    }

    match event_loop.create_window(attributes) {
        Ok(w) => Some(WinitWindow(std::sync::Arc::new(w))),
        Err(e) => {
            tracing::error!(error = %e, "failed to create window");
            None
        }
    }
}

// Applies one winit `WindowEvent` to a single surface's [`EventHandler`] on the same thread. Returns whether
// the surface requested close.
fn dispatch_window_event<H: EventHandler<WinitWindow>>(
    handler: &mut H,
    window: &WinitWindow,
    cursor_position: &mut (f64, f64),
    scale_factor: &mut f64,
    modifiers: &mut platform_core::ModifiersState,
    event: WindowEvent,
) -> WindowEventOutcome {
    match map_window_event(event, cursor_position, scale_factor, modifiers) {
        SurfaceIntent::Event(e) => handler.on_event(e, window),
        SurfaceIntent::Resized(e) => {
            handler.on_event(e, window);
            window.request_redraw();
        }
        SurfaceIntent::Redraw => handler.on_redraw(window),
        SurfaceIntent::Close(e) => {
            handler.on_event(e, window);
            return WindowEventOutcome::CloseRequested;
        }
        SurfaceIntent::Ignore => {}
    }
    WindowEventOutcome::Continue
}

impl Platform for WinitPlatform {
    type Window = WinitWindow;

    fn run<H: EventHandler<Self::Window>>(
        self,
        config: WindowConfig,
        handler: H,
    ) -> Result<(), PlatformError> {
        let mut runner = WinitRunner {
            handler,
            window: None,
            config,
            cursor_position: (0.0, 0.0),
            scale_factor: 1.0,
            modifiers: platform_core::ModifiersState::default(),
            timer_has_fired: false,
            a11y: None,
            a11y_proxy: self.event_loop.create_proxy(),
            a11y_nodes: Vec::new(),
        };
        // The app-facing redraw waker (handed to background threads) wakes the loop through this proxy, not by
        // holding a window — so caching it can't pin a window open.
        let wake_proxy = self.event_loop.create_proxy();
        platform_core::set_loop_waker(std::sync::Arc::new(move || {
            let _ = wake_proxy.send_event(UserEvent::Wake);
        }));
        // Live OS color-scheme changes: winit has no Linux integration, so a portal watch thread pushes them
        // back through the loop via a proxy. Elsewhere winit delivers WindowEvent::ThemeChanged natively.
        #[cfg(target_os = "linux")]
        {
            let proxy = self.event_loop.create_proxy();
            crate::color_scheme::spawn_watch(move |dark| {
                let _ = proxy.send_event(UserEvent::ColorScheme(dark));
            });
        }
        self.event_loop
            .run_app(&mut runner)
            .map_err(|e| PlatformError(e.to_string()))
    }
}

// The initial OS light/dark preference at window creation: winit's native answer (Windows/macOS), falling
// back to the freedesktop portal on Linux where winit always reports `None`.
fn initial_prefers_dark(window: &WinitWindow) -> Option<bool> {
    let winit = window.prefers_dark();
    #[cfg(target_os = "linux")]
    {
        winit.or_else(crate::color_scheme::portal_prefers_dark)
    }
    #[cfg(not(target_os = "linux"))]
    {
        winit
    }
}

// ---- Multi-surface (multi-window) backend --------------------------------------------------------------
//
// M3: every surface shares this one UI thread and one reactive runtime. winit already creates windows and
// pumps their events on the main thread; each surface's `EventHandler` — built here by the factory, carrying
// its own `Surface` world — runs directly on the main thread too. The handler activates its surface around
// every lifecycle call, so the surfaces stay isolated without a thread apiece, and a signal shared between
// them re-runs each surface's effects under its own context. The hardware backend still presents on its own
// per-surface render thread (as in single-window).
//
// Each dispatch is bracketed by the handler's own `new_events`/`about_to_wait` (begin/end of the reactive
// batch), so batch_depth always returns to 0 within one callback — no cross-callback bookkeeping, and a
// surface created in `resumed` (after the iteration's `new_events`) can never leave the batch unbalanced.

// A dynamically-opened surface (`open_surface`) awaiting creation by the running runner. Enqueued from app
// code deep inside an event handler — where `&ActiveEventLoop` (needed to create a winit window) is not
// available — and drained by the runner on its next `about_to_wait`.
struct DynamicRequest {
    config: WindowConfig,
    handler: Box<dyn EventHandler<WinitWindow>>,
    close: Arc<std::sync::atomic::AtomicBool>,
}

thread_local! {
    static DYNAMIC_QUEUE: std::cell::RefCell<Vec<DynamicRequest>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Requests a new top-level window rendering `handler`, created on the next event-loop iteration by the
/// running multi-surface runner (which shares this thread and the one reactive runtime). Returns a flag the
/// caller flips to close the surface. rsx's winit `SurfaceHost` uses this to implement `open_surface` without
/// a per-surface thread. If no multi-surface runner is running, the request simply sits undrained.
pub fn request_dynamic_surface(
    config: WindowConfig,
    handler: Box<dyn EventHandler<WinitWindow>>,
) -> Arc<std::sync::atomic::AtomicBool> {
    let close = Arc::new(std::sync::atomic::AtomicBool::new(false));
    DYNAMIC_QUEUE.with(|q| {
        q.borrow_mut().push(DynamicRequest {
            config,
            handler,
            close: Arc::clone(&close),
        })
    });
    close
}

fn drain_dynamic_requests() -> Vec<DynamicRequest> {
    DYNAMIC_QUEUE.with(|q| std::mem::take(&mut *q.borrow_mut()))
}

// Per-surface main-thread state: the handler plus that surface's input state (needed to translate winit
// events), its last frame-pacing deadline, and — for a dynamically-opened surface — the flag its
// `SurfaceControl` flips to request close.
struct SurfaceRunner {
    handler: Box<dyn EventHandler<WinitWindow>>,
    window: WinitWindow,
    cursor_position: (f64, f64),
    scale_factor: f64,
    modifiers: platform_core::ModifiersState,
    pace: Option<std::time::Duration>,
    // `None` for a statically-declared surface; `Some` for one opened via `open_surface`.
    close_flag: Option<Arc<std::sync::atomic::AtomicBool>>,
    // A dynamically-opened window defers on_resume until its first event, when the compositor has given it
    // its real size (a tiling WM may override the requested size); rendering before that would size the
    // surface and the layout differently. `false` until resumed.
    resumed: bool,
}

// Brings a surface up: build under a panic guard (T-4.2), so a build that fails/panics returns `false` and
// the caller drops it without disturbing the others. Reads the window's *current* size, so calling it once
// the compositor has configured the window keeps the layout and the render surface the same size.
fn resume_surface(surface: &mut SurfaceRunner) -> bool {
    let window = surface.window.clone();
    surface.handler.new_events();
    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if let Some(dark) = initial_prefers_dark(&window) {
            surface
                .handler
                .on_event(Event::ColorSchemeChanged { dark }, &window);
        }
        surface.handler.on_resume(&window)
    }));
    surface.pace = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        surface.handler.about_to_wait()
    }))
    .unwrap_or(None);
    matches!(built, Ok(true))
}

// The runner is non-generic over the handler type: both the statically-declared surfaces (boxed from the
// factory) and the dynamically-opened ones share one `Box<dyn EventHandler<WinitWindow>>` map.
type BoxedFactory = Box<dyn Fn(SurfaceId) -> Box<dyn EventHandler<WinitWindow>>>;

struct WinitMultiRunner {
    factory: BoxedFactory,
    pending: Vec<(SurfaceId, WindowConfig)>,
    surfaces: HashMap<WindowId, SurfaceRunner>,
    created: bool,
    // True only on WaitUntil timer expiry; gates keepalive request_redraw so it fires only on timer ticks.
    timer_has_fired: bool,
}

impl WinitMultiRunner {
    // Creates a window for `handler` and inserts it into the live surface map. `resume_now` brings it up
    // immediately (the initial surfaces, created in `resumed`, whose window winit has already configured);
    // a dynamically-opened surface passes `false` and is resumed on its first event instead (see
    // `window_event`), once the compositor has given it its real size.
    fn spawn_surface(
        &mut self,
        event_loop: &ActiveEventLoop,
        config: WindowConfig,
        handler: Box<dyn EventHandler<WinitWindow>>,
        close_flag: Option<Arc<std::sync::atomic::AtomicBool>>,
        resume_now: bool,
    ) {
        // Creating the window creates its `wl_surface`; hold the GPU lifecycle lock so it can't race another
        // window's render thread present/acquire on the shared Wayland/driver connection. Scoped tightly so
        // `resume_surface` below (which builds the renderer under its own lifecycle lock) isn't nested under it.
        let Some(window) = ({
            let _gpu = renderer_core::gpu_sync::lifecycle_guard();
            create_window_from_config(event_loop, &config)
        }) else {
            return;
        };
        let window_id = window.0.id();
        let mut surface = SurfaceRunner {
            handler,
            window,
            cursor_position: (0.0, 0.0),
            scale_factor: 1.0,
            modifiers: platform_core::ModifiersState::default(),
            pace: None,
            close_flag,
            resumed: false,
        };
        if resume_now {
            if !resume_surface(&mut surface) {
                tracing::error!("surface on_resume failed or panicked; skipping it");
                return;
            }
            surface.window.request_redraw();
            surface.resumed = true;
        }
        self.surfaces.insert(window_id, surface);
    }
}

impl ApplicationHandler<UserEvent> for WinitMultiRunner {
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            // The multi-surface runner attaches no adapter yet, so nothing sends these to it. Its windows are
            // built by a factory after the loop is running, and the adapter has to exist before each is shown —
            // a per-surface hook this backend does not have. Named rather than caught by a wildcard, so adding
            // it is a compile error here and not a silent no-op.
            UserEvent::Accessibility(_) => {}
            #[cfg(target_os = "linux")]
            UserEvent::ColorScheme(dark) => {
                // Bracket each surface's write on its own (this callback is not inside a shared batch bracket).
                for surface in self.surfaces.values_mut() {
                    surface.handler.new_events();
                    surface
                        .handler
                        .on_event(Event::ColorSchemeChanged { dark }, &surface.window);
                    surface.pace = surface.handler.about_to_wait();
                }
            }
            UserEvent::Wake => {
                // Redraw every surface so each one's `on_frame` runs — the waking app's content may now live in
                // any of them (a tabbed host can move it between windows), so we don't assume which.
                for surface in self.surfaces.values() {
                    surface.window.request_redraw();
                }
            }
        }
    }

    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        // Only gate keepalive redraws on a real timer expiry (not every event-queue drain).
        self.timer_has_fired = matches!(cause, StartCause::ResumeTimeReached { .. });
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Bring up any surfaces opened via `open_surface` since the last iteration, and tear down any whose
        // SurfaceControl flag was flipped — both are cheap and immediate on this one thread (no polling).
        for req in drain_dynamic_requests() {
            self.spawn_surface(event_loop, req.config, req.handler, Some(req.close), false);
        }
        let to_close: Vec<WindowId> = self
            .surfaces
            .iter()
            .filter(|(_, s)| {
                s.close_flag
                    .as_ref()
                    .is_some_and(|f| f.load(std::sync::atomic::Ordering::Relaxed))
            })
            .map(|(&id, _)| id)
            .collect();
        for id in to_close {
            if let Some(mut removed) = self.surfaces.remove(&id) {
                removed.handler.on_suspend();
                // Renderer + window teardown, serialized against sibling render threads (see the close path in
                // window_event and renderer_core::gpu_sync).
                let _gpu = renderer_core::gpu_sync::lifecycle_guard();
                drop(removed);
            }
        }
        if self.created && self.surfaces.is_empty() {
            event_loop.exit();
            return;
        }

        // Aggregate the soonest frame-pacing deadline across surfaces; wake each animating surface for its own
        // frame on a timer tick (reactive changes call request_redraw themselves via flush_notify).
        let mut next_wake: Option<std::time::Duration> = None;
        for surface in self.surfaces.values() {
            if let Some(d) = surface.pace {
                if self.timer_has_fired {
                    surface.window.request_redraw();
                }
                next_wake = Some(next_wake.map_or(d, |cur| cur.min(d)));
            }
        }
        match next_wake {
            Some(d) => {
                event_loop.set_control_flow(ControlFlow::WaitUntil(std::time::Instant::now() + d))
            }
            None => event_loop.set_control_flow(ControlFlow::Wait),
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Create every static surface once. winit can emit `resumed` more than once on some platforms; the
        // guard keeps us from spawning duplicate windows.
        if self.created {
            return;
        }
        self.created = true;
        for (id, config) in std::mem::take(&mut self.pending) {
            // The factory gives each handler its own `Surface` world, activated around each lifecycle call.
            let handler = (self.factory)(id);
            self.spawn_surface(event_loop, config, handler, None, true);
        }
        if self.surfaces.is_empty() {
            event_loop.exit();
        }
    }

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(surface) = self.surfaces.get_mut(&id) else {
            return;
        };
        if !surface.resumed {
            // Bring a dynamically-opened window up only on its first non-empty `Resized` — the compositor's
            // configure, when the window has its real size (a tiling WM overrides the requested one). Ignore
            // earlier events (e.g. a pre-configure `RedrawRequested`), which would resume at the requested
            // size and overflow the smaller surface.
            let configured =
                matches!(&event, WindowEvent::Resized(s) if s.width > 0 && s.height > 0);
            if !configured {
                return;
            }
            if resume_surface(surface) {
                surface.resumed = true;
                // Fall through to dispatch this Resized: it carries the authoritative size, which relays the
                // layout out to match the surface even if `window.inner_size()` (read in on_resume) lagged it.
            } else {
                // Exiting the whole loop is `about_to_wait`'s job (it runs after pending `open_surface`
                // requests are spawned), so a just-removed last surface can't kill a window being born.
                if let Some(removed) = self.surfaces.remove(&id) {
                    let _gpu = renderer_core::gpu_sync::lifecycle_guard();
                    drop(removed);
                }
                return;
            }
        }
        // Clone (a cheap Arc bump) so the window borrow doesn't conflict with the mutable handler/input borrows.
        let window = surface.window.clone();
        surface.handler.new_events();
        // Dispatch under a panic guard (T-4.2): a widget handler / render / effect panic unmounts just this
        // surface. about_to_wait (end_batch) is guarded separately so it always runs, keeping the reactive
        // batch balanced (T-1.3 leaves the shared runtime consistent after the unwind). Under panic=unwind
        // only; a panic=abort release build aborts instead.
        let dispatched = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            dispatch_window_event(
                &mut surface.handler,
                &window,
                &mut surface.cursor_position,
                &mut surface.scale_factor,
                &mut surface.modifiers,
                event,
            )
        }));
        let paced = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            surface.handler.about_to_wait()
        }));
        surface.pace = paced.as_ref().copied().unwrap_or(None);
        let panicked = dispatched.is_err() || paced.is_err();
        let close = matches!(dispatched, Ok(WindowEventOutcome::CloseRequested));
        // A panicked handler is in an unknown state; don't poll it, just unmount.
        let exit_requested = !panicked && surface.handler.take_exit_request();
        if panicked {
            tracing::error!(?id, "surface panicked; unmounting it");
        }
        // OS close (WM X / Alt-F4), a custom title-bar close button, or a panic: tear down just this surface.
        if panicked || close || exit_requested {
            if let Some(mut removed) = self.surfaces.remove(&id) {
                // on_suspend joins THIS surface's render thread (which needs the render guard to finish its
                // last frame), so it must run before we take the lifecycle lock — otherwise we'd deadlock.
                if !panicked {
                    removed.handler.on_suspend();
                }
                // Destroying this window's swapchain/surface and its winit window (wl_surface) must not race a
                // sibling window's render thread; the lock waits for every in-flight frame and blocks new ones
                // for the duration of the drop (see renderer_core::gpu_sync). The GPU device/instance is shared
                // process-wide, so this drops only a swapchain — never a device — which is what makes it safe.
                let _gpu = renderer_core::gpu_sync::lifecycle_guard();
                drop(removed);
            }
            tracing::debug!(
                ?id,
                close,
                exit_requested,
                panicked,
                remaining = self.surfaces.len(),
                "surface closed"
            );
            // The whole-loop exit is decided in `about_to_wait`, after the same iteration's queued
            // `open_surface`/`open_window` requests are spawned — so detaching the last tab (which closes the
            // host and opens a new window at once) doesn't exit before the new window exists.
        }
    }
}

impl MultiSurfacePlatform for WinitPlatform {
    type Window = WinitWindow;

    fn run_surfaces<H, F>(
        self,
        surfaces: Vec<(SurfaceId, WindowConfig)>,
        factory: F,
    ) -> Result<(), PlatformError>
    where
        H: EventHandler<WinitWindow> + 'static,
        F: Fn(SurfaceId) -> H + 'static,
    {
        // Box the factory output so static and dynamic surfaces share one handler type in the runner's map.
        let factory: BoxedFactory =
            Box::new(move |id| Box::new(factory(id)) as Box<dyn EventHandler<WinitWindow>>);
        let mut runner = WinitMultiRunner {
            factory,
            pending: surfaces,
            surfaces: HashMap::new(),
            created: false,
            timer_has_fired: false,
        };
        // The app-facing redraw waker wakes the loop through this proxy (which redraws every surface), not by
        // holding a window — so an app can cache it and, if its content is later moved to another window, the
        // original still closes and background wakeups still reach it wherever it now lives.
        let wake_proxy = self.event_loop.create_proxy();
        platform_core::set_loop_waker(std::sync::Arc::new(move || {
            let _ = wake_proxy.send_event(UserEvent::Wake);
        }));
        // Live OS color-scheme changes, delivered to every surface (see the single-window `run`).
        #[cfg(target_os = "linux")]
        {
            let proxy = self.event_loop.create_proxy();
            crate::color_scheme::spawn_watch(move |dark| {
                let _ = proxy.send_event(UserEvent::ColorScheme(dark));
            });
        }
        self.event_loop
            .run_app(&mut runner)
            .map_err(|e| PlatformError(e.to_string()))
    }
}
