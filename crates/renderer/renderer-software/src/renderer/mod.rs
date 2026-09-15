//! The renderer: its pixmap, its surface, and the per-frame state the phases thread between them.

mod frame;
mod pixels;
mod present;
#[cfg(test)]
mod test_frames;
#[cfg(target_os = "linux")]
mod wayland_alpha;

use std::num::NonZeroU32;
use std::sync::mpsc;

use geometry_core::Rect;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use renderer_core::perf::{self, Phase};
use renderer_core::{Color, DrawCommand, RendererError};
use smallvec::SmallVec;
use softbuffer::{Context, Surface};
use tiny_skia::Pixmap;

use pixels::PixelFormat;
use present::{FrameOp, PresentLog, SurfaceDamage, declared_damage, note_damage};
#[cfg(target_os = "android")]
use present::{extract_native_window, present_to_native_window};

/// The CPU backend: a tiny-skia pixmap, presented through softbuffer or read back headless.
pub struct SoftwareRenderer<D: HasDisplayHandle, W: HasWindowHandle> {
    // Kept so the caches can be built on whichever thread ends up driving this renderer: they live in a thread-local, which is not necessarily the thread the constructor ran on.
    config: crate::SoftwareRendererConfig,
    // Building the caches loads fonts, so it is deferred out of the constructor.
    caches_ready: bool,
    // `None` headless: no window means no softbuffer surface, and the frame lives only in `pixmap`.
    _context: Option<Context<D>>,
    surface: Option<Surface<D, W>>,
    width: u32,
    height: u32,
    pub(crate) pixmap: Option<Pixmap>,
    // Read off the shaper by `ensure_caches` so dirty-rect computation does not under-estimate the text region. Holds conservative defaults until then.
    font_metrics: renderer_core::FontMetrics,
    blur_scratch: Vec<u8>,
    pixmap_pool: Vec<tiny_skia::Pixmap>,
    clip_mask_buffer: Option<tiny_skia::Mask>,
    // Tracked across frames, so the next `PushClip` can zero stale bits without re-zeroing the whole mask.
    clip_mask_dirty: Option<Rect>,
    draw_state: renderer_core::DrawState,
    layer_stack: Vec<(tiny_skia::Pixmap, f32, i32, i32)>,
    // Previous frame state for skip-if-identical and dirty-rect optimizations.
    prev_commands: Vec<DrawCommand>,
    prev_commands_hash: u64,
    prev_clear_color: Option<Color>,
    // Cache for expand_fill_layers: avoids re-expanding on idle frames where commands didn't change.
    expanded_commands_cache: Option<(u64, Vec<DrawCommand>)>,
    // Cache for compute_layer_bounds: avoids re-traversing commands when input and dimensions are unchanged.
    layer_bounds_cache: Option<(u64, Vec<Option<(i32, i32, u32, u32)>>)>,
    present_log: PresentLog,
    // Used to present without softbuffer's swizzle and copy. softbuffer still owns surface creation and buffer geometry; this is a second acquired reference used only at present time.
    #[cfg(target_os = "android")]
    native_window: Option<ndk::native_window::NativeWindow>,
    // softbuffer is opaque, so a transparent surface presents via an own `wl_shm` ARGB8888 buffer.
    #[cfg(target_os = "linux")]
    alpha: Option<wayland_alpha::WaylandAlphaPresenter>,
    // Keeps the display and window alive, so the alpha presenter's borrowed pointers stay valid.
    #[cfg(target_os = "linux")]
    _alpha_handles: Option<(D, W)>,
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
        // A transparent Wayland surface cannot use softbuffer, which presents opaque XRGB, so it is driven through an own ARGB8888 `wl_shm` buffer.
        #[cfg(target_os = "linux")]
        let alpha = if config.transparent {
            wayland_alpha::WaylandAlphaPresenter::try_new(&display, &window)
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
            pixmap: None,
            blur_scratch: Vec::new(),
            pixmap_pool: Vec::new(),
            clip_mask_buffer: None,
            clip_mask_dirty: None,
            draw_state: renderer_core::DrawState::new(),
            layer_stack: Vec::new(),
            prev_commands: Vec::with_capacity(256),
            prev_commands_hash: 0,
            prev_clear_color: None,
            expanded_commands_cache: None,
            layer_bounds_cache: None,
            present_log: PresentLog::new(),
            #[cfg(target_os = "android")]
            native_window,
            #[cfg(target_os = "linux")]
            alpha,
            #[cfg(target_os = "linux")]
            _alpha_handles: alpha_handles,
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
            // Pre-sized, so a `begin_frame` at the same dimensions reuses these buffers instead of reallocating.
            width,
            height,
            pixmap: Pixmap::new(width, height),
            blur_scratch: Vec::new(),
            pixmap_pool: Vec::new(),
            clip_mask_buffer: tiny_skia::Mask::new(width, height),
            clip_mask_dirty: None,
            draw_state: renderer_core::DrawState::new(),
            layer_stack: Vec::new(),
            prev_commands: Vec::with_capacity(256),
            prev_commands_hash: 0,
            prev_clear_color: None,
            expanded_commands_cache: None,
            layer_bounds_cache: None,
            present_log: PresentLog::new(),
            #[cfg(target_os = "android")]
            native_window: None,
            #[cfg(target_os = "linux")]
            alpha: None,
            #[cfg(target_os = "linux")]
            _alpha_handles: None,
        }
    }

    /// The current frame's pixels as premultiplied RGBA8888 (tiny-skia byte order: `[R, G, B, A]` per pixel, row-major, `width * height * 4` bytes). `None` before the first frame is rendered.
    pub fn read_rgba(&self) -> Option<&[u8]> {
        self.pixmap.as_ref().map(|p| p.data())
    }

    /// The current frame's backing pixmap (premultiplied RGBA8888). `None` before the first frame is rendered.
    pub fn pixmap(&self) -> Option<&tiny_skia::Pixmap> {
        self.pixmap.as_ref()
    }

    // Returns true if at least one shadow became available this frame.
    fn poll_pending_shadows(&mut self) -> bool {
        let mut arrived = false;
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
            pending_shadows.retain(|key, rx| match rx.try_recv() {
                Ok(pixmap) => {
                    shadow_cache.insert(*key, pixmap);
                    arrived = true;
                    false
                }
                Err(mpsc::TryRecvError::Empty) => true,
                Err(mpsc::TryRecvError::Disconnected) => false,
            });
            pending_text_shadows.retain(|key, rx| match rx.try_recv() {
                Ok(pixmap) => {
                    text_shadow_cache.insert(key.clone(), pixmap);
                    arrived = true;
                    false
                }
                Err(mpsc::TryRecvError::Empty) => true,
                Err(mpsc::TryRecvError::Disconnected) => false,
            });
            pending_path_shadows.retain(|key, rx| match rx.try_recv() {
                Ok(pixmap) => {
                    path_shadow_cache.insert(key.clone(), pixmap);
                    arrived = true;
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
        let Some(pixmap) = &self.pixmap else {
            return Ok(());
        };
        if self.width == 0 || self.height == 0 {
            return Ok(());
        }
        // tiny_skia's RGBA byte order matches the native RGBX8888, so presenting is a per-row memcpy with no swizzle.
        #[cfg(target_os = "android")]
        if let Some(nw) = &self.native_window {
            return present_to_native_window(nw, pixmap);
        }

        // Presents the premultiplied-RGBA frame as ARGB8888, keeping the alpha softbuffer cannot.
        #[cfg(target_os = "linux")]
        if let Some(alpha) = &mut self.alpha {
            let _present = perf::span(Phase::Present);
            self.present_log.record(op);
            alpha.present(
                pixmap.data(),
                self.width,
                self.height,
                &mut self.present_log,
            );
            return Ok(());
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

#[cfg(test)]
#[path = "renderer_test.rs"]
mod tests;
