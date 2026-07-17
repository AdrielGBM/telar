use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};

use platform_core::{
    Event, EventHandler, FullscreenMode, MultiSurfacePlatform, Platform, PlatformError,
    PointerButton, PointerSource, ScrollDelta, SurfaceId, Window, WindowConfig, WindowPosition,
};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseScrollDelta, StartCause, Touch, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::Key as WinitKey;
use winit::window::{Fullscreen, WindowAttributes, WindowId, WindowLevel};

use platform_winit::WinitWindow;

// Winit user-event payloads injected from background threads (via EventLoopProxy) to wake the loop.
enum UserEvent {
    // The OS color-scheme flipped; carries the new dark (`true`) / light preference (Linux portal watch).
    ColorScheme(bool),
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
}

impl<H: EventHandler<WinitWindow>> ApplicationHandler<UserEvent> for WinitRunner<H> {
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::ColorScheme(dark) => {
                if let Some(window) = &self.window {
                    self.handler
                        .on_event(Event::ColorSchemeChanged { dark }, window);
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
        window.request_redraw();
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        // Clone (a cheap Arc bump) so the immutable window borrow doesn't conflict with the mutable
        // self.handler/cursor/scale/modifiers borrows the shared dispatcher takes.
        let Some(window) = self.window.clone() else {
            return;
        };
        let outcome = dispatch_window_event(
            &mut self.handler,
            &window,
            &mut self.cursor_position,
            &mut self.scale_factor,
            &mut self.modifiers,
            event,
        );
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

// What a mapped winit `WindowEvent` means at the platform level, decoupled from *how* it's applied. The
// single-window runner applies it to a handler directly; the multi-window runner forwards it to that
// surface's worker thread. Keeping the mapping here (and the application at the call site) lets both share
// the exact same winit→platform translation.
enum SurfaceIntent {
    // Deliver this platform event to the handler.
    Event(Event),
    // Deliver this platform event, then request a redraw (winit `Resized`).
    Resized(Event),
    // Render now (winit `RedrawRequested`).
    Redraw,
    // Deliver `WindowCloseRequested`, then close this surface.
    Close(Event),
    // State-only (e.g. `ModifiersChanged`) or an unmapped event — nothing to deliver.
    Ignore,
}

// Pure winit `WindowEvent` → [`SurfaceIntent`] translation, updating this surface's cursor/scale/modifiers.
// No handler and no window side effects, so it can run on the winit thread while the handler lives elsewhere.
fn map_window_event(
    event: WindowEvent,
    cursor_position: &mut (f64, f64),
    scale_factor: &mut f64,
    modifiers: &mut platform_core::ModifiersState,
) -> SurfaceIntent {
    match event {
        WindowEvent::CloseRequested => SurfaceIntent::Close(Event::WindowCloseRequested),
        WindowEvent::Resized(size) => SurfaceIntent::Resized(Event::WindowResized {
            width: (size.width as f64 / *scale_factor).round() as u32,
            height: (size.height as f64 / *scale_factor).round() as u32,
        }),
        WindowEvent::RedrawRequested => SurfaceIntent::Redraw,
        WindowEvent::CursorMoved { position, .. } => {
            let lx = position.x / *scale_factor;
            let ly = position.y / *scale_factor;
            *cursor_position = (lx, ly);
            SurfaceIntent::Event(Event::PointerMoved {
                x: lx,
                y: ly,
                source: PointerSource::Mouse,
            })
        }
        WindowEvent::MouseInput { state, button, .. } => {
            let Some(btn) = platform_winit::map_mouse_button(button) else {
                return SurfaceIntent::Ignore;
            };
            let (x, y) = *cursor_position;
            SurfaceIntent::Event(match state {
                ElementState::Pressed => Event::PointerPressed {
                    x,
                    y,
                    button: btn,
                    source: PointerSource::Mouse,
                },
                ElementState::Released => Event::PointerReleased {
                    x,
                    y,
                    button: btn,
                    source: PointerSource::Mouse,
                },
            })
        }
        WindowEvent::Touch(Touch {
            phase,
            location,
            id,
            ..
        }) => {
            let x = location.x / *scale_factor;
            let y = location.y / *scale_factor;
            let source = PointerSource::Touch { id };
            SurfaceIntent::Event(match phase {
                TouchPhase::Started => Event::PointerPressed {
                    x,
                    y,
                    button: PointerButton::Primary,
                    source,
                },
                TouchPhase::Moved => Event::PointerMoved { x, y, source },
                TouchPhase::Ended | TouchPhase::Cancelled => Event::PointerReleased {
                    x,
                    y,
                    button: PointerButton::Primary,
                    source,
                },
            })
        }
        WindowEvent::Focused(is_focused) => {
            SurfaceIntent::Event(Event::FocusChanged { is_focused })
        }
        WindowEvent::CursorEntered { .. } => SurfaceIntent::Event(Event::CursorEntered),
        WindowEvent::CursorLeft { .. } => SurfaceIntent::Event(Event::CursorLeft),
        WindowEvent::ScaleFactorChanged {
            scale_factor: new_scale,
            ..
        } => {
            *scale_factor = new_scale;
            SurfaceIntent::Event(Event::ScaleFactorChanged {
                scale_factor: new_scale,
            })
        }
        WindowEvent::MouseWheel { delta, .. } => {
            let scroll_delta = match delta {
                MouseScrollDelta::LineDelta(x, y) => ScrollDelta::Lines { x, y },
                MouseScrollDelta::PixelDelta(pos) => ScrollDelta::Pixels {
                    x: (pos.x / *scale_factor) as f32,
                    y: (pos.y / *scale_factor) as f32,
                },
            };
            SurfaceIntent::Event(Event::Scrolled {
                delta: scroll_delta,
            })
        }
        WindowEvent::ModifiersChanged(mods) => {
            *modifiers = platform_winit::map_modifiers(&mods);
            SurfaceIntent::Ignore
        }
        WindowEvent::KeyboardInput { event, .. } => {
            let key = match &event.logical_key {
                WinitKey::Character(c) => match c.as_str().chars().next() {
                    Some(ch) => platform_core::Key::Char(ch),
                    None => return SurfaceIntent::Ignore,
                },
                WinitKey::Named(named) => match platform_winit::map_named_key(*named) {
                    Some(nk) => platform_core::Key::Named(nk),
                    None => return SurfaceIntent::Ignore,
                },
                _ => return SurfaceIntent::Ignore,
            };
            let mods = *modifiers;
            SurfaceIntent::Event(match event.state {
                ElementState::Pressed => Event::KeyPressed {
                    key,
                    modifiers: mods,
                },
                ElementState::Released => Event::KeyReleased {
                    key,
                    modifiers: mods,
                },
            })
        }
        WindowEvent::ThemeChanged(theme) => SurfaceIntent::Event(Event::ColorSchemeChanged {
            dark: theme == winit::window::Theme::Dark,
        }),
        _ => SurfaceIntent::Ignore,
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
        };
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
// winit windows are main-thread-bound, but their handles (`WinitWindow` = `Arc<winit::Window>`) are `Send`
// and already used across threads (the hardware render thread). So each surface runs its `EventHandler` — the
// whole reactive/theme/overlay/focus world — on its OWN worker thread, getting a fresh set of thread-locals
// and therefore full isolation with no cross-talk. The winit main thread only creates the windows, pumps OS
// events, and forwards each event to the owning surface's worker over a channel.
//
// Caveat: this path is not GUI-verified in CI (needs a display). On Linux/Wayland/X11 (the shell target)
// building the renderer/surface on the worker thread is fine; macOS would need surface creation on the main
// thread. The hardware backend presents on its own render thread (as in single-window); the software backend
// presents from the worker thread.

// Forwarded from the winit main thread to a surface's worker thread.
enum WorkerMsg {
    // A translated platform input/lifecycle event to dispatch.
    Event(Event),
    // winit asked this window to repaint (`RedrawRequested`).
    Redraw,
    // The OS light/dark preference changed.
    ColorScheme(bool),
    // The window is closing; tear down and exit the worker.
    Close,
}

// The event → reactive → layout → render loop for one surface, run on its own thread. Mirrors the single-
// window runner's iteration shape (`new_events` → dispatch → `on_redraw` → `about_to_wait`) but sourced from
// forwarded messages, and self-paces frames via `recv_timeout` so animations advance without OS events.
fn run_surface_worker<H: EventHandler<WinitWindow>>(
    mut handler: H,
    window: WinitWindow,
    rx: Receiver<WorkerMsg>,
) {
    // Deliver the OS light/dark preference before the tree mounts (mirrors the single-window path).
    if let Some(dark) = initial_prefers_dark(&window) {
        handler.new_events();
        handler.on_event(Event::ColorSchemeChanged { dark }, &window);
        handler.about_to_wait();
    }
    handler.new_events();
    let resumed = handler.on_resume(&window);
    handler.about_to_wait();
    if !resumed {
        tracing::error!("surface worker on_resume failed; window will stay blank");
        return;
    }

    let mut pace: Option<std::time::Duration> = None;
    loop {
        // Block for the next forwarded message, waking on the frame-pacing deadline to drive animations.
        let msg = match pace {
            Some(d) => match rx.recv_timeout(d) {
                Ok(m) => Some(m),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => break,
            },
            None => match rx.recv() {
                Ok(m) => Some(m),
                Err(_) => break,
            },
        };
        handler.new_events();
        match msg {
            Some(WorkerMsg::Event(e)) => handler.on_event(e, &window),
            Some(WorkerMsg::ColorScheme(dark)) => {
                handler.on_event(Event::ColorSchemeChanged { dark }, &window)
            }
            // Redraw and the pacing tick just fall through to on_redraw below.
            Some(WorkerMsg::Redraw) | None => {}
            Some(WorkerMsg::Close) => {
                handler.about_to_wait();
                break;
            }
        }
        // Render (gated internally by tree-dirty / keepalive) after any event or pacing tick.
        handler.on_redraw(&window);
        // A custom title-bar close button on this surface: leave the loop so the window (its only strong ref is
        // this thread) is dropped and closed. Note: the main runner still holds this surface's WorkerHandle, so
        // process-level "exit when the last window closes" is refined in the tabbed-host phase.
        if handler.take_exit_request() {
            break;
        }
        pace = handler.about_to_wait();
    }
    handler.on_suspend();
}

// Main-thread bookkeeping for one surface: the channel to its worker plus that surface's own input state
// (needed to translate winit events, which the worker never sees raw).
struct WorkerHandle {
    tx: Sender<WorkerMsg>,
    join: std::thread::JoinHandle<()>,
    cursor_position: (f64, f64),
    scale_factor: f64,
    modifiers: platform_core::ModifiersState,
}

struct WinitMultiRunner<H, F>
where
    H: EventHandler<WinitWindow>,
    F: Fn(SurfaceId) -> H + Send + Sync + 'static,
{
    factory: Arc<F>,
    pending: Vec<(SurfaceId, WindowConfig)>,
    workers: HashMap<WindowId, WorkerHandle>,
    created: bool,
    _handler: std::marker::PhantomData<fn() -> H>,
}

impl<H, F> ApplicationHandler<UserEvent> for WinitMultiRunner<H, F>
where
    H: EventHandler<WinitWindow>,
    F: Fn(SurfaceId) -> H + Send + Sync + 'static,
{
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::ColorScheme(dark) => {
                for worker in self.workers.values() {
                    let _ = worker.tx.send(WorkerMsg::ColorScheme(dark));
                }
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // The workers self-pace and render on their own threads; the main loop only needs to wake for OS
        // events or a worker's request_redraw, both of which interrupt Wait.
        event_loop.set_control_flow(ControlFlow::Wait);
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Create every surface once. winit can emit `resumed` more than once on some platforms; the guard
        // keeps us from spawning duplicate windows/workers.
        if self.created {
            return;
        }
        self.created = true;
        for (id, config) in std::mem::take(&mut self.pending) {
            let Some(window) = create_window_from_config(event_loop, &config) else {
                continue;
            };
            let window_id = window.0.id();
            let (tx, rx) = channel::<WorkerMsg>();
            let factory = Arc::clone(&self.factory);
            let worker_window = window.clone();
            let join = std::thread::Builder::new()
                .name(format!("rsx-surface-{}", id.0))
                .spawn(move || {
                    // The handler is built here, ON the worker thread, so its reactive/theme/overlay/focus
                    // world lives in this thread's thread-locals — isolated from every other surface.
                    let handler = (factory)(id);
                    run_surface_worker(handler, worker_window, rx);
                })
                .expect("failed to spawn surface worker thread");
            self.workers.insert(
                window_id,
                WorkerHandle {
                    tx,
                    join,
                    cursor_position: (0.0, 0.0),
                    scale_factor: 1.0,
                    modifiers: platform_core::ModifiersState::default(),
                },
            );
        }
        if self.workers.is_empty() {
            event_loop.exit();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(worker) = self.workers.get_mut(&id) else {
            return;
        };
        let intent = map_window_event(
            event,
            &mut worker.cursor_position,
            &mut worker.scale_factor,
            &mut worker.modifiers,
        );
        let close = match intent {
            SurfaceIntent::Event(e) | SurfaceIntent::Resized(e) => {
                let _ = worker.tx.send(WorkerMsg::Event(e));
                false
            }
            SurfaceIntent::Redraw => {
                let _ = worker.tx.send(WorkerMsg::Redraw);
                false
            }
            SurfaceIntent::Close(e) => {
                let _ = worker.tx.send(WorkerMsg::Event(e));
                let _ = worker.tx.send(WorkerMsg::Close);
                true
            }
            SurfaceIntent::Ignore => false,
        };
        if close {
            // Drop the sender (also unblocks the worker's recv) and join so its on_suspend — which tears down
            // the render thread — finishes before the window is dropped. Closing one window drops just that
            // surface; the loop exits when the last one is gone.
            if let Some(worker) = self.workers.remove(&id) {
                drop(worker.tx);
                let _ = worker.join.join();
            }
            if self.workers.is_empty() {
                event_loop.exit();
            }
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
        H: EventHandler<WinitWindow>,
        F: Fn(SurfaceId) -> H + Send + Sync + 'static,
    {
        let mut runner = WinitMultiRunner {
            factory: Arc::new(factory),
            pending: surfaces,
            workers: HashMap::new(),
            created: false,
            _handler: std::marker::PhantomData,
        };
        // Live OS color-scheme changes, delivered to every surface's worker (see the single-window `run`).
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
