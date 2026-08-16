//! Integration test for `spawn_task`: the background-work bridge, driven end-to-end through the real runner.
//!
//! The unit tests in reactive-core cover the queue itself. What this proves is the wiring around it — that the
//! runner drains completions every frame, that the callback runs on the UI thread where it may write a signal
//! the tree is subscribed to, and that the write reaches pixels. A `spawn_task` whose result never landed
//! would leave the window on its initial color.

mod common;

use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use common::assert_center_rgb;
use platform_headless::{FrameSink, HeadlessPlatform};
use telar::NoPaths;
use telar::{
    App, AppConfig, AppCtx, AppPathsProvider, AvailableSpace, Color, Component, Event, EventResult,
    LayoutItem, LayoutStyle, NodeId, ReadSignal, RectStyle, Rectangle, RenderNode, RwSignal,
    SizeDimension, compute_layout, mark_dirty, new_container, reset_layout_runtime,
    run_with_platform, signal, spawn_task,
};

const INITIAL: [u8; 3] = [200, 40, 40];
const FROM_TASK: [u8; 3] = [40, 200, 40];

// Enough frames (paced at 60fps by the headless platform) for a worker returning a constant to finish and be
// drained several times over.
const FRAMES: u32 = 60;

struct SignalFillRoot {
    root: NodeId,
    rect: Rectangle,
}

impl SignalFillRoot {
    fn new(color: ReadSignal<Color>) -> Self {
        // Read inside the style closure, which `view()` re-runs under the segment's effect: that subscription
        // is what turns the task's signal write into a repaint.
        let rect = Rectangle::new(
            LayoutStyle::new()
                .width(SizeDimension::Percent(1.0))
                .height(SizeDimension::Percent(1.0)),
            move || RectStyle::filled(color.get(), 0.0),
        )
        .unwrap();
        let root = new_container(
            LayoutStyle::new()
                .width(SizeDimension::Percent(1.0))
                .height(SizeDimension::Percent(1.0)),
            &[rect.layout_node()],
        )
        .unwrap();
        Self { root, rect }
    }
}

impl Component for SignalFillRoot {
    fn view(&self) -> RenderNode {
        self.rect.view()
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        if let Event::WindowResized { width, height } = event {
            mark_dirty(self.root).ok();
            compute_layout(
                self.root,
                AvailableSpace::Definite(*width as f32),
                AvailableSpace::Definite(*height as f32),
            )
            .ok();
            return EventResult::Handled;
        }
        EventResult::Ignored
    }
}

/// Fills its window with a color that only a background task can produce.
struct TaskApp {
    color: RefCell<Option<RwSignal<Color>>>,
    spawned: bool,
    ran_on_ui_thread: Arc<Mutex<Option<std::thread::ThreadId>>>,
}

impl App for TaskApp {
    fn root(&self) -> Box<dyn Component> {
        reset_layout_runtime();
        let color = signal(Color::from_rgb_u8(INITIAL[0], INITIAL[1], INITIAL[2]));
        let read = color.read_only();
        *self.color.borrow_mut() = Some(color);
        Box::new(SignalFillRoot::new(read))
    }

    fn clear_color(&self) -> Option<Color> {
        Some(Color::rgba(0.0, 0.0, 0.0, 1.0))
    }

    fn on_frame(&mut self, _ctx: &mut AppCtx) {
        if self.spawned {
            return;
        }
        self.spawned = true;
        // The signal is `!Send` and is captured by the completion callback alone — the value is what crosses.
        let color = self.color.borrow().clone().expect("root() ran first");
        let observed = Arc::clone(&self.ran_on_ui_thread);
        spawn_task(
            || FROM_TASK,
            move |rgb| {
                *observed.lock().unwrap() = Some(std::thread::current().id());
                color.set(Color::from_rgb_u8(rgb[0], rgb[1], rgb[2]));
            },
        );
    }
}

#[test]
fn a_background_task_result_reaches_the_screen() {
    let (w, h) = (32u32, 24u32);
    let sink: FrameSink = Arc::new(Mutex::new(None));
    let callback_thread: Arc<Mutex<Option<std::thread::ThreadId>>> = Arc::new(Mutex::new(None));

    let platform = HeadlessPlatform::new(w, h)
        .with_frames(FRAMES)
        .capture_into(sink.clone());

    let ui_thread = std::thread::current().id();
    run_with_platform::<_, _, ()>(
        platform,
        AppConfig::default(),
        Box::new(NoPaths) as Box<dyn AppPathsProvider>,
        TaskApp {
            color: RefCell::new(None),
            spawned: false,
            ran_on_ui_thread: Arc::clone(&callback_thread),
        },
        "telar-headless-spawn-task",
    )
    .expect("headless run failed");

    assert_eq!(
        *callback_thread.lock().unwrap(),
        Some(ui_thread),
        "the completion callback must run on the UI thread, not the worker"
    );

    let pixels = sink.lock().unwrap().take().expect("no frame was captured");
    assert_center_rgb(&pixels, w, h, FROM_TASK, "color delivered by spawn_task");
}
