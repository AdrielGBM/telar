//! [`RenderBackend`]: what a renderer must do, and the factory a frontend installs to build one.

use crate::{Color, DrawCommand, FontConfig, RendererError};

/// What every backend does with a frame: begin it, draw a command list, present it.
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
    /// The most recently rendered frame as premultiplied RGBA8888 (`[R, G, B, A]` per pixel, row-major), if this backend renders to an offscreen target. Windowed/on-screen backends present directly and return `None`. Used to read back pixels from a headless render pass.
    fn read_rgba(&self) -> Option<Vec<u8>> {
        None
    }

    /// Called once on the thread that will drive this backend, before its first frame.
    ///
    /// A backend built on the UI thread and then moved to a render thread has to re-establish whatever per-thread state its constructor set up there. The software rasteriser keeps its glyph shaper and shadow caches in a thread-local, so without this the render thread finds an empty slot and builds a default one — with no font config. On desktop that silently falls back to system fonts; on Android there are none to find and cosmic-text aborts the process with "no default font found".
    fn bind_to_render_thread(&mut self) {}

    /// How long the render thread should go without a frame before calling [`sweep_idle_caches`](Self::sweep_idle_caches). `None` (the default) means never.
    ///
    /// Exists because a backend's caches may be thread-local: they then belong to the render thread, and nothing on the UI thread can reach them — so the sweep has to be driven from the thread that owns them, and only that thread knows when it has been idle.
    fn idle_sweep_after(&self) -> Option<std::time::Duration> {
        None
    }

    /// Drops cache entries no frame has asked for within their idle horizon. Called once per idle stretch, on the render thread, after [`idle_sweep_after`](Self::idle_sweep_after) has elapsed with no frame.
    fn sweep_idle_caches(&mut self) {}

    /// Whether this backend applies `begin_frame`'s `scale_factor` itself — the hardware path folds it into the shader's transform. A backend that returns `false` (the default, and what the software rasteriser does) must be handed commands already scaled into physical pixels, which is why the frame pipeline runs [`ScaleScratch`](crate::ScaleScratch) for it.
    fn applies_scale_factor(&self) -> bool {
        false
    }
}

// Lets an installed renderer travel the frame pipeline, which is generic over `R: RenderBackend + Send` so it can own a concrete backend and hand it back on join.
impl RenderBackend for Box<dyn RenderBackend + Send> {
    fn begin_frame(
        &mut self,
        width: u32,
        height: u32,
        scale_factor: f32,
        generation: u64,
    ) -> Result<(), RendererError> {
        (**self).begin_frame(width, height, scale_factor, generation)
    }

    fn render_frame(
        &mut self,
        commands: &[DrawCommand],
        clear_color: Option<Color>,
    ) -> Result<(), RendererError> {
        (**self).render_frame(commands, clear_color)
    }

    fn read_rgba(&self) -> Option<Vec<u8>> {
        (**self).read_rgba()
    }

    fn bind_to_render_thread(&mut self) {
        (**self).bind_to_render_thread()
    }

    fn idle_sweep_after(&self) -> Option<std::time::Duration> {
        (**self).idle_sweep_after()
    }

    fn sweep_idle_caches(&mut self) {
        (**self).sweep_idle_caches()
    }

    fn applies_scale_factor(&self) -> bool {
        (**self).applies_scale_factor()
    }
}

/// What a renderer is built from, beyond the surface it draws on.
pub struct RendererBuild<'a> {
    /// The faces the app's text is shaped with — the same set the layout-time measurer was configured with, since measure and draw have to agree on what a string is as wide as.
    pub fonts: &'a FontConfig,
    /// Whether the app asked for a transparent surface. A renderer is *built* for one or the other.
    pub transparent: bool,
}

/// A renderer that has been built, and where it may be driven from.
///
/// The distinction is not a preference: a renderer built on top of a browser.s WebGPU device holds JavaScript objects, which are `!Send` by construction and cannot leave the thread that made them. A backend that says so is driven inline on the UI thread; one that can move gets a render thread of its own.
pub enum BuiltRenderer {
    /// Free to be moved to a render thread, which is where the frame pipeline puts it.
    Threaded(Box<dyn RenderBackend + Send>),
    /// Bound to the thread that built it, and driven there.
    Inline(Box<dyn RenderBackend>),
}

impl From<Box<dyn RenderBackend + Send>> for BuiltRenderer {
    fn from(backend: Box<dyn RenderBackend + Send>) -> Self {
        Self::Threaded(backend)
    }
}

/// Builds the renderer for a surface — the seam an out-of-tree frontend installs to draw Telar.s frames itself.
///
/// Generic over the window type because that is the platform.s business: whoever brings a `Platform` brings the window this draws on.
pub trait RendererFactory<W>: 'static {
    fn build(&self, window: &W, build: RendererBuild<'_>) -> Result<BuiltRenderer, RendererError>;

    /// Whether this renderer draws text by shaping glyphs from font files.
    ///
    /// `false` for a backend whose text is somebody else's to draw — a terminal writes the characters and lets the terminal emulator pick the face. Saying so keeps the runtime from scanning the system font directories on startup for a renderer that will never open one of them.
    fn shapes_text(&self) -> bool {
        true
    }
}
