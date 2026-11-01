//! An app driven end to end on a window with no OS handles at all, drawing through a renderer installed from outside the runtime.
//!
//! The shape a terminal frontend has, and unreachable twice over before these seams: `Window` demanded `raw-window-handle` handles a terminal cannot produce, and the runner named its renderers outright. Both halves are asserted here on the real frame pipeline — render thread, stale-frame gate, buffer recycling and all.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use renderer_record::{Recording, RecordingFactory};
use telar::{
    App, AppConfig, Color, Component, DrawCommand, EventHandler, LayoutStyle, NoPaths, Platform,
    PlatformError, RectStyle, Rectangle, Text, TextStyle, Window, WindowConfig,
};

// The handler paces the frame pass at 60fps off a wall clock, so a faster loop is declined and draws nothing.
const FRAME_BUDGET: Duration = Duration::from_nanos(1_000_000_000 / 60);

/// A window with no `raw-window-handle` impls whatsoever. It implements [`Window`] and stops there, which is the point: nothing about hosting an app requires a surface a GPU could bind to.
#[derive(Clone)]
struct CellWindow {
    redraws: Arc<AtomicU32>,
}

impl Window for CellWindow {
    fn width(&self) -> u32 {
        320
    }

    fn height(&self) -> u32 {
        200
    }

    fn request_redraw(&self) {
        self.redraws.fetch_add(1, Ordering::SeqCst);
    }
}

/// Drives the handler through one resume, a few frames and a suspend, in the iteration shape the winit and headless backends use, so the reactive batching brackets stay balanced.
struct CellPlatform {
    frames: u32,
}

impl Platform for CellPlatform {
    type Window = CellWindow;

    fn run<H: EventHandler<CellWindow>>(
        self,
        _config: WindowConfig,
        mut handler: H,
    ) -> Result<(), PlatformError> {
        let window = CellWindow {
            redraws: Arc::new(AtomicU32::new(0)),
        };
        handler.new_events();
        if !handler.on_resume(&window) {
            return Err(PlatformError(
                "on_resume refused the installed renderer".to_string(),
            ));
        }
        handler.about_to_wait();
        for _ in 0..self.frames {
            handler.new_events();
            std::thread::sleep(FRAME_BUDGET);
            handler.on_redraw(&window);
            handler.about_to_wait();
        }
        // Joins the render thread, so every frame it accepted is in the recording by the time `run` returns.
        handler.on_suspend();
        Ok(())
    }
}

struct Panel {
    /// What the app was told about its OS handles, recorded from `on_frame` because that is where an app asks.
    saw_handles: Arc<AtomicBool>,
}

impl App for Panel {
    fn root(&self) -> Box<dyn Component> {
        telar::reset_layout_runtime();
        let label = Text::new(
            || "installed".to_string(),
            LayoutStyle::new(),
            || TextStyle::new(14.0, Color::BLACK),
        )
        .expect("a label builds");
        let box_ = Rectangle::new(LayoutStyle::new().width(40.0).height(20.0), || {
            RectStyle::filled(Color::BLACK, 0.0)
        })
        .expect("a rectangle builds");
        Box::new(
            telar::Container::new(LayoutStyle::new(), telar::children![label, box_])
                .expect("a container builds"),
        )
    }

    fn on_frame(&mut self, ctx: &mut telar::AppCtx) {
        if ctx.raw_window_handle().is_some() || ctx.raw_display_handle().is_some() {
            self.saw_handles.store(true, Ordering::SeqCst);
        }
    }
}

#[test]
fn an_installed_renderer_draws_an_app_on_a_window_with_no_os_handles() {
    let recording = Recording::new();
    let saw_handles = Arc::new(AtomicBool::new(false));

    telar::run_with_platform_and_renderer::<CellPlatform, _, _, ()>(
        CellPlatform { frames: 3 },
        RecordingFactory::new(recording.clone()),
        AppConfig::default(),
        std::sync::Arc::new(NoPaths),
        Panel {
            saw_handles: Arc::clone(&saw_handles),
        },
        "installed-renderer-test",
    )
    .expect("the app runs");

    let frame = recording
        .last_frame()
        .expect("the installed renderer was handed at least one frame");
    assert_eq!(
        (frame.width, frame.height),
        (320, 200),
        "the surface size reaching the renderer is the window's own"
    );
    assert!(
        frame
            .commands
            .iter()
            .any(|c| matches!(c, DrawCommand::Rect { .. })),
        "the app's rectangle never reached the renderer: {:?}",
        frame.commands
    );
    // Text is the half that has to be measured before it can be drawn, so this covers the metrics seam too.
    assert!(
        frame
            .commands
            .iter()
            .any(|c| matches!(c, DrawCommand::Text { .. })),
        "the app's label never reached the renderer: {:?}",
        frame.commands
    );
    assert!(
        !saw_handles.load(Ordering::SeqCst),
        "a window with no handles must report none, not something invented"
    );
}

#[test]
fn a_recording_is_readable_after_the_render_thread_is_gone() {
    let recording = Recording::new();
    let frames = Arc::new(Mutex::new(0usize));

    telar::run_with_platform_and_renderer::<CellPlatform, _, _, ()>(
        CellPlatform { frames: 2 },
        RecordingFactory::new(recording.clone()),
        AppConfig::default(),
        std::sync::Arc::new(NoPaths),
        Panel {
            saw_handles: Arc::new(AtomicBool::new(false)),
        },
        "installed-renderer-test",
    )
    .expect("the app runs");

    *frames.lock().unwrap() = recording.frame_count();
    assert!(
        *frames.lock().unwrap() >= 1,
        "the recorder is shared, so the frames it took on the render thread survive the join"
    );
}
