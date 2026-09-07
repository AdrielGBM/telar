use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;
use web_time::Instant;

use geometry_core::Rect;
use renderer_core::{DrawCommand, RectStyle, RendererError};

use super::*;

/// Stands in for a real backend so the pipeline itself can be tested: what it was asked to draw, at what size, and how many frames actually reached it.
struct StubBackend {
    scales_itself: bool,
    rendered: Arc<AtomicU32>,
    report: std::sync::mpsc::Sender<Drawn>,
    size: (u32, u32),
    bound: bool,
    bound_before_first_frame: Arc<AtomicBool>,
    idle_sweep_after: Option<Duration>,
    sweeps: Arc<AtomicU32>,
}

impl RenderBackend for StubBackend {
    fn applies_scale_factor(&self) -> bool {
        self.scales_itself
    }

    fn bind_to_render_thread(&mut self) {
        self.bound = true;
    }

    fn idle_sweep_after(&self) -> Option<Duration> {
        self.idle_sweep_after
    }

    fn sweep_idle_caches(&mut self) {
        self.sweeps.fetch_add(1, Ordering::SeqCst);
    }

    fn begin_frame(
        &mut self,
        width: u32,
        height: u32,
        _scale_factor: f32,
        _generation: u64,
    ) -> Result<(), RendererError> {
        if self.rendered.load(Ordering::SeqCst) == 0 {
            self.bound_before_first_frame
                .store(self.bound, Ordering::SeqCst);
        }
        self.size = (width, height);
        Ok(())
    }

    fn render_frame(
        &mut self,
        commands: &[DrawCommand],
        _clear: Option<renderer_core::Color>,
    ) -> Result<(), RendererError> {
        self.rendered.fetch_add(1, Ordering::SeqCst);
        let _ = self
            .report
            .send((self.size.0, self.size.1, commands.to_vec()));
        Ok(())
    }
}

fn rect(x: f32) -> DrawCommand {
    DrawCommand::Rect {
        rect: Rect::new(x, 0.0, 10.0, 10.0),
        style: Arc::new(RectStyle::default()),
    }
}

fn frame(width: u32, height: u32, scale_factor: f32, age: Duration) -> FrameMsg {
    FrameMsg {
        width,
        height,
        scale_factor,
        generation: 0,
        commands: vec![rect(20.0)],
        clear: None,
        timestamp: Instant::now() - age,
    }
}

/// What one call to `render_frame` was handed: the surface size in force, and the commands themselves.
type Drawn = (u32, u32, Vec<DrawCommand>);

fn stub(
    scales_itself: bool,
) -> (
    StubBackend,
    Arc<AtomicU32>,
    std::sync::mpsc::Receiver<Drawn>,
) {
    let (backend, rendered, seen, _) = stub_watching_bind(scales_itself);
    (backend, rendered, seen)
}

fn stub_watching_bind(
    scales_itself: bool,
) -> (
    StubBackend,
    Arc<AtomicU32>,
    std::sync::mpsc::Receiver<Drawn>,
    Arc<AtomicBool>,
) {
    let rendered = Arc::new(AtomicU32::new(0));
    let bound_before_first_frame = Arc::new(AtomicBool::new(false));
    let (report, seen) = std::sync::mpsc::channel();
    (
        StubBackend {
            scales_itself,
            rendered: Arc::clone(&rendered),
            report,
            size: (0, 0),
            bound: false,
            bound_before_first_frame: Arc::clone(&bound_before_first_frame),
            idle_sweep_after: None,
            sweeps: Arc::new(AtomicU32::new(0)),
        },
        rendered,
        seen,
        bound_before_first_frame,
    )
}

fn x_of(command: &DrawCommand) -> f32 {
    match command {
        DrawCommand::Rect { rect, .. } => rect.x,
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn a_backend_that_does_not_scale_is_handed_scaled_commands() {
    let (backend, _rendered, seen) = stub(false);
    let (tx, _ret_rx, join) = spawn_render_thread(backend);

    tx.send(frame(100, 50, 2.0, Duration::ZERO)).unwrap();
    let (_, _, commands) = seen.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(
        x_of(&commands[0]),
        40.0,
        "20px at scale 2 is 40 physical px"
    );

    drop(tx);
    join.join().unwrap();
}

// Hardware folds the scale into its shader transform, so pre-scaling would apply it twice.
#[test]
fn a_backend_that_scales_itself_is_handed_logical_commands() {
    let (backend, _rendered, seen) = stub(true);
    let (tx, _ret_rx, join) = spawn_render_thread(backend);

    tx.send(frame(100, 50, 2.0, Duration::ZERO)).unwrap();
    let (_, _, commands) = seen.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(
        x_of(&commands[0]),
        20.0,
        "left in logical px for the shader"
    );

    drop(tx);
    join.join().unwrap();
}

#[test]
fn a_stale_frame_is_dropped_and_its_buffer_recycled() {
    let (backend, rendered, seen) = stub(false);
    let (tx, ret_rx, join) = spawn_render_thread(backend);

    tx.send(frame(100, 50, 1.0, Duration::ZERO)).unwrap();
    seen.recv_timeout(Duration::from_secs(5)).unwrap();
    let _ = ret_rx.recv_timeout(Duration::from_secs(5)).unwrap();

    tx.send(frame(100, 50, 1.0, Duration::from_millis(500)))
        .unwrap();
    let recycled = ret_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(recycled.len(), 1, "the buffer comes back for refilling");
    assert_eq!(
        rendered.load(Ordering::SeqCst),
        1,
        "a frame older than the budget must not be drawn"
    );

    drop(tx);
    join.join().unwrap();
}

// Skipping a resize would leave the surface at the old size, so the window shows clipped content until some later frame happens to be accepted.
#[test]
fn a_stale_frame_that_resizes_is_drawn_anyway() {
    let (backend, rendered, seen) = stub(false);
    let (tx, _ret_rx, join) = spawn_render_thread(backend);

    tx.send(frame(100, 50, 1.0, Duration::ZERO)).unwrap();
    seen.recv_timeout(Duration::from_secs(5)).unwrap();
    tx.send(frame(640, 480, 1.0, Duration::from_millis(500)))
        .unwrap();
    let (w, h, _) = seen.recv_timeout(Duration::from_secs(5)).unwrap();

    assert_eq!((w, h), (640, 480));
    assert_eq!(rendered.load(Ordering::SeqCst), 2);

    drop(tx);
    join.join().unwrap();
}

// Regression: the software rasteriser reached its first string with a shaper that had been handed no fonts, which on Android aborts the process outright.
#[test]
fn the_backend_is_bound_to_the_thread_before_the_first_frame() {
    let (backend, _rendered, seen, bound_first) = stub_watching_bind(false);
    let (tx, _ret_rx, join) = spawn_render_thread(backend);

    tx.send(frame(100, 50, 1.0, Duration::ZERO)).unwrap();
    seen.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(
        bound_first.load(Ordering::SeqCst),
        "bind_to_render_thread must run before the first begin_frame"
    );

    drop(tx);
    join.join().unwrap();
}

#[test]
fn an_idle_render_thread_sweeps_its_own_caches_once() {
    let (mut backend, _rendered, seen, _) = stub_watching_bind(false);
    backend.idle_sweep_after = Some(Duration::from_millis(30));
    let sweeps = Arc::clone(&backend.sweeps);
    let (tx, _ret_rx, join) = spawn_render_thread(backend);

    tx.send(frame(100, 50, 1.0, Duration::ZERO)).unwrap();
    seen.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(sweeps.load(Ordering::SeqCst), 0, "not while frames arrive");

    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        sweeps.load(Ordering::SeqCst),
        1,
        "one sweep per idle stretch, not a repeating timer"
    );

    tx.send(frame(100, 50, 1.0, Duration::ZERO)).unwrap();
    seen.recv_timeout(Duration::from_secs(5)).unwrap();

    drop(tx);
    join.join().unwrap();
}

#[test]
fn joining_hands_the_renderer_back() {
    let (backend, _rendered, seen) = stub(false);
    let (tx, _ret_rx, join) = spawn_render_thread(backend);

    tx.send(frame(320, 240, 1.0, Duration::ZERO)).unwrap();
    seen.recv_timeout(Duration::from_secs(5)).unwrap();
    drop(tx);

    let recovered = join.join().expect("render thread panicked");
    assert_eq!(recovered.size, (320, 240), "state survived the join");
}
