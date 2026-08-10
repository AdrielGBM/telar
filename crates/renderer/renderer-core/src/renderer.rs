use crate::{Color, DrawCommand, RendererError};

pub trait RenderBackend {
    /// Begin a new frame. Note: `scale_factor` and `generation` may be ignored by backends that receive pre-scaled commands (see `SoftwareRenderer::begin_frame`).
    fn begin_frame(
        &mut self,
        width: u32,
        height: u32,
        scale_factor: f32,
        generation: u64,
    ) -> Result<(), RendererError>;
    /// Process and present all draw commands for this frame. Must be called exactly once per frame after `begin_frame`.
    fn render_frame(
        &mut self,
        commands: &[DrawCommand],
        clear_color: Option<Color>,
    ) -> Result<(), RendererError>;
    /// The most recently rendered frame as premultiplied RGBA8888 (`[R, G, B, A]` per pixel, row-major), if
    /// this backend renders to an offscreen target. Windowed/on-screen backends present directly and return
    /// `None`. Used to read back pixels from a headless render pass.
    fn read_rgba(&self) -> Option<Vec<u8>> {
        None
    }

    /// Called once on the thread that will drive this backend, before its first frame.
    ///
    /// A backend built on the UI thread and then moved to a render thread has to re-establish whatever
    /// per-thread state its constructor set up there. The software rasteriser keeps its glyph shaper and
    /// shadow caches in a thread-local, so without this the render thread finds an empty slot and builds a
    /// default one — with no font config. On desktop that silently falls back to system fonts; on Android
    /// there are none to find and cosmic-text aborts the process with "no default font found".
    fn bind_to_render_thread(&mut self) {}

    /// How long the render thread should go without a frame before calling
    /// [`sweep_idle_caches`](Self::sweep_idle_caches). `None` (the default) means never.
    ///
    /// Exists because a backend's caches may be thread-local: they then belong to the render thread, and
    /// nothing on the UI thread can reach them — so the sweep has to be driven from the thread that owns
    /// them, and only that thread knows when it has been idle.
    fn idle_sweep_after(&self) -> Option<std::time::Duration> {
        None
    }

    /// Drops cache entries no frame has asked for within their idle horizon. Called once per idle stretch,
    /// on the render thread, after [`idle_sweep_after`](Self::idle_sweep_after) has elapsed with no frame.
    fn sweep_idle_caches(&mut self) {}

    /// Whether this backend applies `begin_frame`'s `scale_factor` itself — the hardware path folds it into
    /// the shader's transform. A backend that returns `false` (the default, and what the software rasteriser
    /// does) must be handed commands already scaled into physical pixels, which is why the frame pipeline
    /// runs [`ScaleScratch`](crate::ScaleScratch) for it.
    fn applies_scale_factor(&self) -> bool {
        false
    }
}
