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
            #[cfg(all(feature = "android-bare", target_os = "android"))]
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
                #[cfg(all(feature = "android-bare", target_os = "android"))]
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
                #[cfg(all(feature = "android-bare", target_os = "android"))]
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
#[path = "frame_thread_test.rs"]
mod tests;
