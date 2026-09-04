//! The headless runner: no compositor, frames paced by a clock, for tests and offscreen rendering.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use platform_core::{
    EventHandler, MultiSurfacePlatform, Platform, PlatformError, SurfaceId, WindowConfig,
};

use crate::window::HeadlessWindow;

// AppHandler paces content frames off a real wall clock at 60fps; the run loop waits out this budget before each redraw so the frame actually rasterizes instead of being deferred by the pacing gate.
const FRAME_BUDGET: Duration = Duration::from_nanos(1_000_000_000 / 60);

/// A shared slot the platform writes the final frame's pixels into. `Platform::run` yields no value, so a caller that wants the rendered pixels passes one of these via [`HeadlessPlatform::capture_into`] and reads it after `run` returns.
pub type FrameSink = Arc<Mutex<Option<Vec<u8>>>>;

/// The multi-surface analogue of [`FrameSink`]: each surface's final frame keyed by its [`SurfaceId`]. Passed via [`HeadlessPlatform::capture_surfaces_into`] and read after [`MultiSurfacePlatform::run_surfaces`] returns.
pub type SurfaceFrameSink = Arc<Mutex<HashMap<SurfaceId, Vec<u8>>>>;

/// A first-class, windowless [`Platform`] backend: it drives the exact same [`EventHandler`] seam as the winit backend (`on_resume` → `on_redraw`s → `on_suspend`) against a [`HeadlessWindow`], with no event loop, GPU swapchain, or display server. Because the handler builds an offscreen renderer for a headless window, this routes a *real* app end-to-end (reactive → layout → render → pixels) and is both the reference `Platform` impl and a deterministic integration-test harness.
///
/// Construct it with the surface size and optionally a frame count and a sink to capture the final frame's pixels; then drive it via [`crate::run`-style entry points] — e.g. `telar::run_with_platform(HeadlessPlatform::new(w, h).capture_into(sink), …)`.
pub struct HeadlessPlatform {
    width: u32,
    height: u32,
    frames: u32,
    sink: Option<FrameSink>,
    surface_sink: Option<SurfaceFrameSink>,
}

impl HeadlessPlatform {
    /// A `width`×`height` offscreen surface at scale 1.0, one render frame.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            frames: 1,
            sink: None,
            surface_sink: None,
        }
    }

    /// How many render frames to drive. Defaults to 1. Use more to let animations or multi-pass reactive settling converge before the final pixels are captured.
    pub fn with_frames(mut self, frames: u32) -> Self {
        self.frames = frames;
        self
    }

    /// Capture the final frame's premultiplied RGBA8 pixels into `sink`, readable after `run` returns.
    pub fn capture_into(mut self, sink: FrameSink) -> Self {
        self.sink = Some(sink);
        self
    }

    /// Capture each surface's final frame into `sink`, keyed by [`SurfaceId`], readable after [`MultiSurfacePlatform::run_surfaces`] returns. Only consulted by the multi-surface path.
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
        let window = HeadlessWindow::with_options(self.width, self.height, 1.0, None);

        // Mirror the winit loop's iteration shape (new_events → dispatch → about_to_wait) so the handler's reactive batching brackets stay balanced exactly as they do under winit.
        handler.new_events();
        let resumed = handler.on_resume(&window);
        handler.about_to_wait();
        if !resumed {
            return Err(PlatformError(
                "headless on_resume returned false (renderer initialization failed)".to_string(),
            ));
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
        H: EventHandler<HeadlessWindow> + 'static,
        F: Fn(SurfaceId) -> H + 'static,
    {
        // Every surface shares this thread and one reactive runtime. The handler factory gives each its own `Surface` world, activated around every lifecycle call, so they stay isolated without a thread apiece.
        let frames = self.frames.max(1);
        let sink = self.surface_sink.clone();

        // Build every handler and window up front, on this thread.
        let mut states: Vec<(SurfaceId, HeadlessWindow, H)> = Vec::with_capacity(surfaces.len());
        for (id, config) in surfaces {
            let window = HeadlessWindow::new(config.width, config.height);
            states.push((id, window, factory(id)));
        }

        // A surface whose renderer fails or whose build panics is dropped rather than being fatal to the run. The `new_events`/`about_to_wait` bracket keeps the reactive batch balanced even then, since `end_batch` runs regardless. Effective only under `panic=unwind`.
        states.retain_mut(|(id, window, handler)| {
            handler.new_events();
            let resumed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                handler.on_resume(window)
            }));
            let _ =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handler.about_to_wait()));
            if resumed.is_err() {
                eprintln!("surface {} panicked during build; unmounting it", id.0);
            }
            matches!(resumed, Ok(true))
        });

        // Drive the scripted frame count: pace once per round (so every surface's frame budget has elapsed and its redraw actually rasterizes), then redraw every surface. A surface that panics mid-frame is unmounted so the rest keep rendering.
        for _ in 0..frames {
            std::thread::sleep(FRAME_BUDGET);
            states.retain_mut(|(id, window, handler)| {
                handler.new_events();
                let drawn = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    handler.on_redraw(window)
                }));
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    handler.about_to_wait()
                }));
                if drawn.is_err() {
                    eprintln!("surface {} panicked during redraw; unmounting it", id.0);
                }
                drawn.is_ok()
            });
        }

        if let Some(sink) = &sink {
            for (id, _, handler) in &mut states {
                if let Some(pixels) = handler.last_frame_rgba() {
                    sink.lock().unwrap().insert(*id, pixels);
                }
            }
        }

        for (_, _, handler) in &mut states {
            handler.on_suspend();
        }
        Ok(())
    }
}
