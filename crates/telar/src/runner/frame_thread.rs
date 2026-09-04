//! The frame pipeline's render thread: the worker every backend hands its composed commands to.
//!
//! Lives here rather than in `hot_host`, which is compiled only under `dev` and named for hot reload — this is the always-compiled core of the frame loop, and `handler.rs` imports it on every build.

use renderer_core::RenderBackend;

use super::FRAME_BUDGET;

pub(super) struct FrameMsg {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) scale_factor: f32,
    pub(super) generation: u64,
    pub(super) commands: Vec<renderer_core::DrawCommand>,
    pub(super) clear: Option<renderer_core::Color>,
    pub(super) timestamp: web_time::Instant,
}

/// Drives `renderer` on a thread of its own, fed one [`FrameMsg`] at a time.
///
/// Generic over the backend so the software rasteriser gets the same pipeline the hardware one has always had — stale-frame dropping, buffer recycling, `catch_unwind`, and an ADPF session keyed to the thread that actually does the work. Staying generic (rather than boxing) is what lets `on_suspend` join and reclaim the *concrete* renderer, which is how the hardware path keeps its device, pipelines and caches warm.
///
/// **Frames here are droppable.** Anything added to this loop has to tolerate a frame never arriving: the stale-frame gate below skips whole frames whenever the UI thread outruns the renderer, so no step may leave a side effect half-applied for the next one to finish.
///
/// Only the UI-thread side of the boundary is `!Send`-constrained; nothing reactive crosses. What arrives is flat data plus `Arc`s, and the proof of that is simply that this compiles.
pub(super) fn spawn_render_thread<R>(
    renderer: R,
) -> (
    std::sync::mpsc::SyncSender<FrameMsg>,
    std::sync::mpsc::Receiver<Vec<renderer_core::DrawCommand>>,
    std::thread::JoinHandle<R>,
)
where
    R: RenderBackend + Send + 'static,
{
    let (tx, rx) = std::sync::mpsc::sync_channel::<FrameMsg>(1);
    // Handed back to the UI thread so it refills the same allocation instead of allocating a fresh Vec each frame.
    let (ret_tx, ret_rx) = std::sync::mpsc::channel::<Vec<renderer_core::DrawCommand>>();
    let join = std::thread::Builder::new()
        .name("telar-render".to_string())
        .spawn(move || {
            let mut renderer = renderer;
            // Whatever per-thread state the constructor set up on the UI thread has to exist here too, or the first frame finds it empty and improvises.
            renderer.bind_to_render_thread();
            let mut current_width = 0u32;
            let mut current_height = 0u32;
            // For backends that do not fold it into a shader, kept off the UI thread: on the software path it is the largest per-frame cost that would otherwise sit in front of input.
            let mut scale_scratch = renderer_core::ScaleScratch::new();
            let scales_itself = renderer.applies_scale_factor();
            // The hint session must carry this thread's own TID, so `reportActualWorkDuration` drives the scheduler for the thread that submits the work. It is not `Send`, so it is created, used and dropped here.
            #[cfg(target_os = "android")]
            let hint_session = platform_android::AdpfSession::new(16_666_667, None);
            let idle_sweep_after = renderer.idle_sweep_after();
            loop {
                // One sweep per idle stretch, then park on a plain `recv`: a repeating timer would wake this thread forever on a screen nobody is looking at.
                let msg = match idle_sweep_after {
                    Some(after) => match rx.recv_timeout(after) {
                        Ok(msg) => msg,
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                            renderer.sweep_idle_caches();
                            match rx.recv() {
                                Ok(msg) => msg,
                                Err(_) => break,
                            }
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    },
                    None => match rx.recv() {
                        Ok(msg) => msg,
                        Err(_) => break,
                    },
                };
                // Never skip a frame that resizes: the surface is reconfigured inside `begin_frame`, so dropping one leaves it at the old size and the window shows clipped content until the next accepted frame.
                let size_changed = msg.width != current_width || msg.height != current_height;
                if !size_changed && msg.timestamp.elapsed() > FRAME_BUDGET {
                    let _ = ret_tx.send(msg.commands);
                    continue;
                }
                #[cfg(target_os = "android")]
                let frame_start = web_time::Instant::now();
                // `begin_frame` reconfigures the swapchain, and a wgpu fatal error there is a panic rather than an `Err`, so catch it and drop the frame instead of unwinding into an abort.
                let began = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    renderer.begin_frame(msg.width, msg.height, msg.scale_factor, msg.generation)
                }));
                if !matches!(began, Ok(Ok(()))) {
                    let _ = ret_tx.send(msg.commands);
                    continue;
                }
                current_width = msg.width;
                current_height = msg.height;
                // A wgpu validation error is fatal by default and would abort the process from this render thread, so the frame is dropped and the app recovers on the next correctly-sized one. Recovery needs `panic=unwind`, which the consuming binary's profile decides.
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let commands: &[renderer_core::DrawCommand] =
                        if scales_itself || msg.scale_factor == 1.0 {
                            &msg.commands
                        } else {
                            scale_scratch.scale_into(&msg.commands, msg.scale_factor)
                        };
                    renderer.render_frame(commands, msg.clear)
                }));
                #[cfg(target_os = "android")]
                if let Some(session) = &hint_session {
                    let duration_ns = frame_start.elapsed().as_nanos() as i64;
                    session.report(duration_ns);
                }
                // Recycled for the UI thread to refill; a send failure just drops it.
                let _ = ret_tx.send(msg.commands);
            }
            // `hint_session` drops here, on this thread, before it exits. The renderer is returned so `on_suspend` can reclaim it and keep warm caches across resume.
            renderer
        })
        .expect("failed to spawn render thread");
    (tx, ret_rx, join)
}

#[cfg(test)]
mod tests {
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
}
