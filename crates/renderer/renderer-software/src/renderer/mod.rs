//! The renderer: its pixmap, its surface, and the per-frame state the phases thread between them.

mod clip;
mod frame;
mod pixels;
mod present;
#[cfg(target_os = "linux")]
mod swapchain;
#[cfg(test)]
mod test_frames;
#[cfg(target_os = "linux")]
mod wayland_alpha;

use std::num::NonZeroU32;
use std::sync::mpsc;

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use renderer_core::dirty::FrameDiff;
use renderer_core::perf::{self, Phase};
use renderer_core::{Color, DrawCommand, RendererError};
use smallvec::SmallVec;
use softbuffer::{Context, Surface};
use tiny_skia::{Pixmap, PixmapMut};

use clip::{ClipMask, ClipShape};
use frame::{Layer, Regions};
use pixels::{LayerBox, PixelFormat};
use present::{FrameOp, PresentLog, SurfaceDamage, declared_damage, note_damage};
#[cfg(target_os = "android")]
use present::{extract_native_window, present_to_native_window};
#[cfg(target_os = "linux")]
use swapchain::AlphaPresenter;

// A compositor releases a buffer it has replaced within a frame or two, so the first retry comes soon after; the doubling bounds the wakes, about 13 s in all, for one that holds it far longer.
#[cfg(target_os = "linux")]
const IDLE_RETRY: std::time::Duration = std::time::Duration::from_millis(50);
#[cfg(target_os = "linux")]
const IDLE_RETRIES: u32 = 8;

/// The CPU backend: a tiny-skia pixmap, presented through softbuffer or read back headless.
pub struct SoftwareRenderer<D: HasDisplayHandle, W: HasWindowHandle> {
    // Kept so the caches can be built on whichever thread ends up driving this renderer: they live in a thread-local, which is not necessarily the thread the constructor ran on.
    config: crate::SoftwareRendererConfig,
    // Building the caches loads fonts, so it is deferred out of the constructor.
    caches_ready: bool,
    // `None` headless, where the frame lives only in the target, and on a transparent Wayland surface.
    _context: Option<Context<D>>,
    surface: Option<Surface<D, W>>,
    width: u32,
    height: u32,
    pub(crate) target: Target,
    // Read off the shaper by `ensure_caches` so dirty-rect computation does not under-estimate the text region. Holds conservative defaults until then.
    font_metrics: renderer_core::FontMetrics,
    blur_scratch: Vec<u8>,
    pixmap_pool: Vec<tiny_skia::Pixmap>,
    mask_pool: Vec<ClipMask>,
    // Surface-sized, so allocated on the first draw that needs one and dropped when the window goes idle.
    clip_mask: Option<ClipMask>,
    // Masks every unclipped draw on the surface too: tiny-skia blends an unmasked draw through a pipeline that rounds differently, and a repaint has to match the frame it patches pixel for pixel.
    damage_mask: Option<ClipMask>,
    draw_state: renderer_core::DrawState,
    // Each open clip as it was pushed, since `draw_state` keeps only the rects and only intersected.
    clip_shapes: Vec<ClipShape>,
    layer_stack: Vec<Layer>,
    frame_diff: FrameDiff,
    // Previous frame state for skip-if-identical and dirty-rect optimizations.
    prev_commands: Vec<DrawCommand>,
    prev_commands_hash: u64,
    prev_clear_color: Option<Color>,
    // Cache for expand_fill_layers: avoids re-expanding on idle frames where commands didn't change.
    expanded_commands_cache: Option<(u64, Vec<DrawCommand>)>,
    // Cache for compute_layer_bounds: avoids re-traversing commands when input and dimensions are unchanged.
    layer_bounds_cache: Option<(u64, Vec<Option<LayerBox>>)>,
    present_log: PresentLog,
    // Used to present without softbuffer's swizzle and copy. softbuffer still owns surface creation and buffer geometry; this is a second acquired reference used only at present time.
    #[cfg(target_os = "android")]
    native_window: Option<ndk::native_window::NativeWindow>,
    // Keeps the display and window alive, so the alpha presenter's borrowed pointers stay valid.
    #[cfg(target_os = "linux")]
    _alpha_handles: Option<(D, W)>,
    // Retries of the idle release this idle stretch, while the compositor holds a buffer it would free.
    #[cfg(target_os = "linux")]
    idle_retries: u32,
    #[cfg(test)]
    fail_after: Option<usize>,
}

impl<D, W> SoftwareRenderer<D, W>
where
    D: HasDisplayHandle,
    W: HasWindowHandle,
{
    pub fn new(
        display: D,
        window: W,
        config: crate::SoftwareRendererConfig,
    ) -> Result<Self, RendererError> {
        // A transparent Wayland surface cannot use softbuffer, which presents opaque XRGB, so it is driven through its own `wl_shm` buffers.
        #[cfg(target_os = "linux")]
        let alpha = if config.transparent {
            wayland_alpha::try_new(&display, &window)
                .map(|presenter| Box::new(presenter) as Box<dyn AlphaPresenter>)
        } else {
            None
        };
        #[cfg(target_os = "linux")]
        let use_alpha = alpha.is_some();
        #[cfg(not(target_os = "linux"))]
        let use_alpha = false;

        let context;
        let surface;
        #[cfg(target_os = "linux")]
        let alpha_handles;
        #[cfg(target_os = "android")]
        let native_window;

        if use_alpha {
            context = None;
            surface = None;
            #[cfg(target_os = "linux")]
            {
                alpha_handles = Some((display, window));
            }
            #[cfg(target_os = "android")]
            {
                native_window = None;
            }
        } else {
            let ctx = Context::new(display).map_err(|e| {
                RendererError::Backend(format!("softbuffer context creation failed: {}", e))
            })?;
            // Acquired before `window` is moved into softbuffer; used to present without its intermediate buffer.
            #[cfg(target_os = "android")]
            {
                native_window = extract_native_window(&window);
            }
            surface = Some(
                Surface::new(&ctx, window).map_err(|e| RendererError::Surface(e.to_string()))?,
            );
            context = Some(ctx);
            #[cfg(target_os = "linux")]
            {
                alpha_handles = None;
            }
        }

        // softbuffer is opaque on every platform, and only the Wayland `wl_shm` path bypasses it, so a transparent surface renders opaque here. Surfaced rather than failing silently — the hardware backend gives transparency everywhere. Extending this needs a per-OS bypass, addable only where it can be tested.
        if config.transparent && !use_alpha {
            tracing::warn!(
                "software renderer: transparent surfaces are only supported on Linux/Wayland; this surface will be opaque. Use the hardware backend for transparency on this platform."
            );
        }

        Ok(Self {
            config,
            caches_ready: false,
            // A conservative placeholder until `ensure_caches` reads the real thing off the shaper. Nothing reads it before the first frame, which runs `ensure_caches` before it draws.
            font_metrics: renderer_core::FontMetrics::default(),
            _context: context,
            surface,
            width: 0,
            height: 0,
            target: Target {
                pixmap: None,
                #[cfg(target_os = "linux")]
                alpha,
            },
            blur_scratch: Vec::new(),
            pixmap_pool: Vec::new(),
            mask_pool: Vec::new(),
            clip_mask: None,
            damage_mask: None,
            draw_state: renderer_core::DrawState::new(),
            clip_shapes: Vec::new(),
            layer_stack: Vec::new(),
            frame_diff: FrameDiff::default(),
            prev_commands: Vec::with_capacity(256),
            prev_commands_hash: 0,
            prev_clear_color: None,
            expanded_commands_cache: None,
            layer_bounds_cache: None,
            present_log: PresentLog::new(),
            #[cfg(target_os = "android")]
            native_window,
            #[cfg(target_os = "linux")]
            _alpha_handles: alpha_handles,
            #[cfg(target_os = "linux")]
            idle_retries: 0,
            #[cfg(test)]
            fail_after: None,
        })
    }

    /// Builds this thread's glyph shaper and shadow caches, and reads the real font metrics off the shaper.
    ///
    /// Deferred out of the constructors on purpose: the caches are a thread-local and building one loads fonts, so doing it in `new` would build them on the thread that *made* the renderer rather than the one that will draw with it. For an on-screen surface those are different threads, and the UI thread's copy would then sit unused for the life of the process.
    fn ensure_caches(&mut self) {
        if self.caches_ready {
            return;
        }
        crate::caches::init(&self.config);
        self.font_metrics = crate::caches::with_caches(|c| c.text_shaper.font_metrics());
        self.caches_ready = true;
    }

    /// Builds an offscreen renderer with no window: rendering targets an in-memory `Pixmap` only, so no display server, softbuffer context, or surface is required (snapshot tests, server-side render, benchmarks). The `D`/`W` type parameters are never instantiated — the caller picks any concrete types. Read the result back with [`read_rgba`](Self::read_rgba) or [`pixmap`](Self::pixmap).
    pub fn new_headless(width: u32, height: u32, config: crate::SoftwareRendererConfig) -> Self {
        Self {
            config,
            caches_ready: false,
            font_metrics: renderer_core::FontMetrics::default(),
            _context: None,
            surface: None,
            // Pre-sized, so a `begin_frame` at the same dimensions reuses the pixmap instead of reallocating.
            width,
            height,
            target: Target {
                pixmap: Pixmap::new(width, height),
                #[cfg(target_os = "linux")]
                alpha: None,
            },
            blur_scratch: Vec::new(),
            pixmap_pool: Vec::new(),
            mask_pool: Vec::new(),
            clip_mask: None,
            damage_mask: None,
            draw_state: renderer_core::DrawState::new(),
            clip_shapes: Vec::new(),
            layer_stack: Vec::new(),
            frame_diff: FrameDiff::default(),
            prev_commands: Vec::with_capacity(256),
            prev_commands_hash: 0,
            prev_clear_color: None,
            expanded_commands_cache: None,
            layer_bounds_cache: None,
            present_log: PresentLog::new(),
            #[cfg(target_os = "android")]
            native_window: None,
            #[cfg(target_os = "linux")]
            _alpha_handles: None,
            #[cfg(target_os = "linux")]
            idle_retries: 0,
            #[cfg(test)]
            fail_after: None,
        }
    }

    /// The current frame's pixels as premultiplied RGBA8888 (tiny-skia byte order: `[R, G, B, A]` per pixel, row-major, `width * height * 4` bytes). `None` before the first frame is rendered.
    pub fn read_rgba(&self) -> Option<&[u8]> {
        self.target.rgba()
    }

    /// The current frame's backing pixmap (premultiplied RGBA8888). `None` before the first frame is rendered, and on a surface whose frames are drawn straight into its present buffers, which [`read_rgba`](Self::read_rgba) still reads.
    pub fn pixmap(&self) -> Option<&tiny_skia::Pixmap> {
        self.target.pixmap.as_ref()
    }

    // Drops every surface-sized buffer only a frame in flight uses; the next frame allocates what it needs again. Returns when to try again for a present buffer the compositor still holds.
    fn release_idle_buffers(&mut self) -> Option<std::time::Duration> {
        self.clip_mask = None;
        self.damage_mask = None;
        self.mask_pool = Vec::new();
        self.pixmap_pool = Vec::new();
        self.blur_scratch = Vec::new();
        #[cfg(target_os = "linux")]
        if let Some(alpha) = &mut self.target.alpha
            && !alpha.release_idle()
            && self.idle_retries < IDLE_RETRIES
        {
            let retry = IDLE_RETRY * 2u32.pow(self.idle_retries);
            self.idle_retries += 1;
            return Some(retry);
        }
        None
    }

    // What a frame that unwound part way left behind is neither the last frame nor the next, so the next one is drawn whole, into a buffer nothing trusts.
    fn abandon_frame(&mut self) {
        self.forget_previous_frame();
        self.layer_stack.clear();
        #[cfg(target_os = "linux")]
        if let Some(alpha) = &mut self.target.alpha {
            alpha.abandon();
        }
    }

    #[cfg(test)]
    fn fail_after_commands(&mut self) {
        match &mut self.fail_after {
            Some(0) => {
                self.fail_after = None;
                panic!("a draw failed part way through the frame");
            }
            Some(left) => *left -= 1,
            None => {}
        }
    }

    // Where the shadows that finished blurring this frame paint, one region each and nothing for a worker that failed.
    fn poll_pending_shadows(&mut self) -> Regions {
        let mut arrived = Regions::new();
        // Destructured so each `retain` and the cache it drains into are disjoint borrows of the shared set.
        crate::caches::with_caches(|c| {
            let crate::caches::SharedCaches {
                shadow_cache,
                pending_shadows,
                text_shadow_cache,
                pending_text_shadows,
                path_shadow_cache,
                pending_path_shadows,
                ..
            } = c;
            pending_shadows.retain(|key, waiting| match waiting.arrived() {
                Ok((pixmap, footprint)) => {
                    shadow_cache.insert(*key, pixmap);
                    arrived.push(footprint);
                    false
                }
                Err(mpsc::TryRecvError::Empty) => true,
                Err(mpsc::TryRecvError::Disconnected) => false,
            });
            pending_text_shadows.retain(|key, waiting| match waiting.arrived() {
                Ok((pixmap, footprint)) => {
                    text_shadow_cache.insert(key.clone(), pixmap);
                    arrived.push(footprint);
                    false
                }
                Err(mpsc::TryRecvError::Empty) => true,
                Err(mpsc::TryRecvError::Disconnected) => false,
            });
            pending_path_shadows.retain(|key, waiting| match waiting.arrived() {
                Ok((pixmap, footprint)) => {
                    path_shadow_cache.insert(key.clone(), pixmap);
                    arrived.push(footprint);
                    false
                }
                Err(mpsc::TryRecvError::Empty) => true,
                Err(mpsc::TryRecvError::Disconnected) => false,
            });
        });
        arrived
    }

    // `op` describes how this frame's pixmap differs from the previous one, so a surface buffer is refreshed and damaged only where it changed. Pass `FrameOp::Full` when unsure.
    fn present_pixmap(&mut self, op: FrameOp) -> Result<(), RendererError> {
        if self.width == 0 || self.height == 0 {
            return Ok(());
        }
        // Keeps the alpha softbuffer cannot, committing the buffer the frame was drawn into or converting the pixmap into one.
        #[cfg(target_os = "linux")]
        if let Some(alpha) = &mut self.target.alpha {
            let _present = perf::span(Phase::Present);
            self.present_log.record(op);
            // Nothing new since the last present is already on screen, and committing it again would only take a buffer.
            if matches!(self.present_log.pending(), FrameOp::NoChange) {
                return Ok(());
            }
            if alpha.draws_in_place() {
                alpha.present(&mut self.present_log);
            } else if let Some(pixmap) = &self.target.pixmap {
                alpha.present_converted(
                    pixmap.data(),
                    self.width,
                    self.height,
                    &mut self.present_log,
                );
            }
            return Ok(());
        }

        let Some(pixmap) = &self.target.pixmap else {
            return Ok(());
        };
        // tiny_skia's RGBA byte order matches the native RGBX8888, so presenting is a per-row memcpy with no swizzle.
        #[cfg(target_os = "android")]
        if let Some(nw) = &self.native_window {
            return present_to_native_window(nw, pixmap);
        }

        // Headless: the frame already lives in `self.pixmap`, so presenting is a no-op.
        let Some(surface) = &mut self.surface else {
            return Ok(());
        };
        let _present = perf::span(Phase::Present);
        self.present_log.record(op);
        let Ok(mut buffer) = surface.buffer_mut() else {
            return Ok(());
        };
        let plan = self.present_log.plan(buffer.age());
        {
            let _convert = perf::span(Phase::Convert);
            plan.refresh(
                pixmap.data(),
                &mut buffer,
                self.width as usize,
                self.height as usize,
                PixelFormat::Xrgb8888,
            );
        }

        let surface_damage = if self.config.retains_presented_contents {
            SurfaceDamage::Rects
        } else {
            SurfaceDamage::Whole
        };
        let rects = declared_damage(&plan.changed, surface_damage, self.width, self.height);
        note_damage(rects.as_deref(), self.width, self.height);
        match rects {
            Some(rects) => {
                let damage: SmallVec<[softbuffer::Rect; 8]> = rects
                    .iter()
                    .filter_map(|r| {
                        Some(softbuffer::Rect {
                            x: r.x,
                            y: r.y,
                            width: NonZeroU32::new(r.width)?,
                            height: NonZeroU32::new(r.height)?,
                        })
                    })
                    .collect();
                buffer.present_with_damage(&damage)
            }
            None => buffer.present(),
        }
        .map_err(|e| RendererError::Present(e.to_string()))?;
        self.present_log.presented();
        Ok(())
    }
}

#[cfg(all(test, target_os = "linux"))]
impl<D, W> SoftwareRenderer<D, W>
where
    D: HasDisplayHandle,
    W: HasWindowHandle,
{
    fn presenting_through(width: u32, height: u32, presenter: Box<dyn AlphaPresenter>) -> Self {
        let mut renderer = Self::new_headless(width, height, Default::default());
        if presenter.draws_in_place() {
            renderer.target.pixmap = None;
        }
        renderer.target.alpha = Some(presenter);
        renderer
    }
}

// Where frames are drawn: a pixmap the present copies or converts, or, where the compositor takes tiny-skia's byte order, the present buffers themselves, which keep the last frame as the pixmap otherwise would.
pub(crate) struct Target {
    pub(crate) pixmap: Option<Pixmap>,
    #[cfg(target_os = "linux")]
    alpha: Option<Box<dyn AlphaPresenter>>,
}

impl Target {
    fn draws_in_place(&self) -> bool {
        #[cfg(target_os = "linux")]
        if let Some(alpha) = &self.alpha {
            return alpha.draws_in_place();
        }
        false
    }

    // On the in-place path, only between `bind` and the present, while a buffer is picked.
    fn pixmap_mut(&mut self) -> Option<PixmapMut<'_>> {
        #[cfg(target_os = "linux")]
        if let Some(alpha) = self.alpha.as_mut().filter(|alpha| alpha.draws_in_place()) {
            return alpha.target();
        }
        self.pixmap.as_mut().map(Pixmap::as_mut)
    }

    fn rgba(&self) -> Option<&[u8]> {
        #[cfg(target_os = "linux")]
        if let Some(alpha) = self.alpha.as_ref().filter(|alpha| alpha.draws_in_place()) {
            return alpha.presented();
        }
        self.pixmap.as_ref().map(Pixmap::data)
    }
}

#[cfg(test)]
#[path = "renderer_test.rs"]
mod tests;
