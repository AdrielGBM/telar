use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use platform_core::{
    Event, EventHandler, MultiSurfacePlatform, Platform, PlatformError, SurfaceId, WindowConfig,
};

use crate::window::HeadlessWindow;

// AppHandler paces content frames off a real wall clock at 60fps; the run loop waits out this budget before
// each redraw so the frame actually rasterizes instead of being deferred by the pacing gate.
const FRAME_BUDGET: Duration = Duration::from_nanos(1_000_000_000 / 60);

/// A shared slot the platform writes the final frame's pixels into. `Platform::run` yields no value, so a
/// caller that wants the rendered pixels passes one of these via [`HeadlessPlatform::capture_into`] and reads
/// it after `run` returns.
pub type FrameSink = Arc<Mutex<Option<Vec<u8>>>>;

/// The multi-surface analogue of [`FrameSink`]: each surface's final frame keyed by its [`SurfaceId`]. Passed
/// via [`HeadlessPlatform::capture_surfaces_into`] and read after [`MultiSurfacePlatform::run_surfaces`]
/// returns.
pub type SurfaceFrameSink = Arc<Mutex<HashMap<SurfaceId, Vec<u8>>>>;

/// A first-class, windowless [`Platform`] backend: it drives the exact same [`EventHandler`] seam as the winit
/// backend (`on_resume` → scripted `on_event`s → `on_redraw`s → `on_suspend`) against a [`HeadlessWindow`],
/// with no event loop, GPU swapchain, or display server. Because the handler builds an offscreen renderer for
/// a headless window, this routes a *real* app end-to-end (event → reactive → layout → render → pixels) and is
/// both the reference `Platform` impl and a deterministic integration-test harness.
///
/// Construct it with the surface size, optionally script input events and a frame count, and optionally
/// capture the final frame's pixels; then drive it via [`crate::run`-style entry points] — e.g.
/// `rsx::run_with_platform(HeadlessPlatform::new(w, h).capture_into(sink), …)`.
pub struct HeadlessPlatform {
    width: u32,
    height: u32,
    scale_factor: f64,
    prefers_dark: Option<bool>,
    events: Vec<Event>,
    frames: u32,
    sink: Option<FrameSink>,
    surface_sink: Option<SurfaceFrameSink>,
}

impl HeadlessPlatform {
    /// A `width`×`height` offscreen surface at scale 1.0, no scripted events, one render frame.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            scale_factor: 1.0,
            prefers_dark: None,
            events: Vec::new(),
            frames: 1,
            sink: None,
            surface_sink: None,
        }
    }

    /// Report a HiDPI scale factor to the app (drives logical-vs-physical sizing).
    pub fn with_scale_factor(mut self, scale_factor: f64) -> Self {
        self.scale_factor = scale_factor;
        self
    }

    /// Report an OS light/dark preference (`Some(true)` = dark) before the tree mounts.
    pub fn with_prefers_dark(mut self, prefers_dark: Option<bool>) -> Self {
        self.prefers_dark = prefers_dark;
        self
    }

    /// Scripted input events delivered (in order) after `on_resume`, each as its own loop iteration.
    pub fn with_events(mut self, events: Vec<Event>) -> Self {
        self.events = events;
        self
    }

    /// How many render frames to drive after the scripted events. Defaults to 1. Use more to let animations or
    /// multi-pass reactive settling converge before the final pixels are captured.
    pub fn with_frames(mut self, frames: u32) -> Self {
        self.frames = frames;
        self
    }

    /// Capture the final frame's premultiplied RGBA8 pixels into `sink`, readable after `run` returns.
    pub fn capture_into(mut self, sink: FrameSink) -> Self {
        self.sink = Some(sink);
        self
    }

    /// Capture each surface's final frame into `sink`, keyed by [`SurfaceId`], readable after
    /// [`MultiSurfacePlatform::run_surfaces`] returns. Only consulted by the multi-surface path.
    pub fn capture_surfaces_into(mut self, sink: SurfaceFrameSink) -> Self {
        self.surface_sink = Some(sink);
        self
    }
}

impl Platform for HeadlessPlatform {
    type Window = HeadlessWindow;

    fn run<H: EventHandler<HeadlessWindow>>(
        self,
        _config: WindowConfig,
        mut handler: H,
    ) -> Result<(), PlatformError> {
        let window = HeadlessWindow::with_options(
            self.width,
            self.height,
            self.scale_factor,
            self.prefers_dark,
        );

        // Mirror the winit loop's iteration shape (new_events → dispatch → about_to_wait) so the handler's
        // reactive batching brackets stay balanced exactly as they do under winit.
        handler.new_events();
        let resumed = handler.on_resume(&window);
        handler.about_to_wait();
        if !resumed {
            return Err(PlatformError(
                "headless on_resume returned false (renderer initialization failed)".to_string(),
            ));
        }

        for event in self.events {
            handler.new_events();
            handler.on_event(event, &window);
            handler.about_to_wait();
        }

        for _ in 0..self.frames.max(1) {
            handler.new_events();
            std::thread::sleep(FRAME_BUDGET);
            handler.on_redraw(&window);
            handler.about_to_wait();
        }

        if let Some(sink) = &self.sink
            && let Some(pixels) = handler.last_frame_rgba()
        {
            *sink.lock().unwrap() = Some(pixels);
        }

        handler.on_suspend();
        Ok(())
    }
}

impl MultiSurfacePlatform for HeadlessPlatform {
    type Window = HeadlessWindow;

    fn run_surfaces<H, F>(
        self,
        surfaces: Vec<(SurfaceId, WindowConfig)>,
        factory: F,
    ) -> Result<(), PlatformError>
    where
        H: EventHandler<HeadlessWindow>,
        F: Fn(SurfaceId) -> H + Send + Sync + 'static,
    {
        // Each surface runs on its own thread, so it gets a fresh thread-local reactive/theme/overlay/focus
        // world — full isolation with no cross-talk, and no runtime-activation juggling. The handler `H` is
        // built by the factory *inside* its thread and never crosses a thread boundary, so it need not be Send.
        let frames = self.frames.max(1);
        let factory = Arc::new(factory);
        let sink = self.surface_sink.clone();

        let mut joins = Vec::with_capacity(surfaces.len());
        for (id, config) in surfaces {
            let factory = Arc::clone(&factory);
            let sink = sink.clone();
            let join = std::thread::Builder::new()
                .name(format!("rsx-headless-surface-{}", id.0))
                .spawn(move || {
                    let window = HeadlessWindow::new(config.width, config.height);
                    let mut handler = factory(id);
                    handler.new_events();
                    let resumed = handler.on_resume(&window);
                    handler.about_to_wait();
                    if !resumed {
                        return;
                    }
                    for _ in 0..frames {
                        handler.new_events();
                        std::thread::sleep(FRAME_BUDGET);
                        handler.on_redraw(&window);
                        handler.about_to_wait();
                    }
                    if let Some(sink) = &sink
                        && let Some(pixels) = handler.last_frame_rgba()
                    {
                        sink.lock().unwrap().insert(id, pixels);
                    }
                    handler.on_suspend();
                })
                .map_err(|e| PlatformError(format!("failed to spawn surface thread: {e}")))?;
            joins.push(join);
        }

        for join in joins {
            join.join()
                .map_err(|_| PlatformError("a surface thread panicked".to_string()))?;
        }
        Ok(())
    }
}
