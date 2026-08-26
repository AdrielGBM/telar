mod frame;
mod pixels;
mod present;
#[cfg(target_os = "linux")]
mod wayland_alpha;

use std::sync::mpsc;

use geometry_core::Rect;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use renderer_core::{Color, DrawCommand, RendererError};
use softbuffer::{Context, Surface};
use tiny_skia::Pixmap;

#[cfg(target_endian = "little")]
use pixels::{convert_rgba_to_xrgb, convert_rgba_to_xrgb_region};
#[cfg(target_endian = "little")]
use present::PresentPlan;
use present::{FrameOp, plan_present};
#[cfg(target_os = "android")]
use present::{extract_native_window, present_to_native_window};

pub struct SoftwareRenderer<D: HasDisplayHandle, W: HasWindowHandle> {
    // Kept so the caches can be built on whichever thread ends up driving this renderer: they live in a
    // thread-local, and that is not necessarily the thread the constructor ran on.
    config: crate::SoftwareRendererConfig,
    // Building the caches loads fonts, so it is deferred out of the constructor — see `ensure_caches`.
    caches_ready: bool,
    // None in headless mode: no window means no softbuffer context/surface; the frame lives only in `pixmap`.
    _context: Option<Context<D>>,
    surface: Option<Surface<D, W>>,
    width: u32,
    height: u32,
    pub(crate) pixmap: Option<Pixmap>,
    // Real font ascender/line-height metrics for the default face, read off the shaper by `ensure_caches` so dirty-rect computation does not under-estimate the text region. Holds conservative defaults until then.
    font_metrics: renderer_core::FontMetrics,
    blur_scratch: Vec<u8>,
    pixmap_pool: Vec<tiny_skia::Pixmap>,
    clip_mask_buffer: Option<tiny_skia::Mask>,
    // Last region written as 0xFF into clip_mask_buffer. Tracked across frames so the next PushClip can zero stale bits left by the previous frame without re-zeroing the whole mask.
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
    // Per-frame change log for the damage-aware present path; an aged softbuffer buffer is brought current by replaying the last `age` entries instead of re-swizzling the whole framebuffer. Bounded to the last few frames.
    present_history: std::collections::VecDeque<FrameOp>,
    // Android only: a direct handle to the surface's ANativeWindow, used to present without softbuffer's swizzle+copy. softbuffer still owns surface creation and buffer-geometry; this is a second acquired reference used only at present time.
    #[cfg(target_os = "android")]
    native_window: Option<ndk::native_window::NativeWindow>,
    // Linux only: when the app asked for a transparent surface, present via an own `wl_shm` ARGB8888 buffer, since softbuffer is opaque. `None` means the opaque softbuffer path is in use.
    #[cfg(target_os = "linux")]
    alpha: Option<wayland_alpha::WaylandAlphaPresenter>,
    // Keeps the display/window alive so the alpha presenter's borrowed `wl_display`/`wl_surface` pointers stay valid for the renderer's lifetime.
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
        // A transparent Wayland surface can't use softbuffer (it presents opaque XRGB): drive it through an own ARGB8888 `wl_shm` buffer that preserves alpha. Every other case uses softbuffer as before.
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
            // Acquire a direct ANativeWindow reference before `window` is moved into softbuffer; used to present without softbuffer's intermediate buffer.
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

        // A transparent surface the software backend can't honor here (softbuffer is opaque on every platform; only the Linux/Wayland `wl_shm` ARGB path bypasses it) renders opaque. Surface it instead of failing silently — the hardware backend gives transparency on every platform. TODO: extend software transparency beyond Linux/Wayland — each OS needs its own softbuffer bypass, addable only when that platform is available to test: Windows (WS_EX_LAYERED + UpdateLayeredWindow, premultiplied ARGB DIB), macOS (non-opaque NSWindow + CALayer alpha), Linux/X11 (32-bit ARGB visual + compositor), Android (RGBA8888 ANativeWindow instead of RGBX).
        if config.transparent && !use_alpha {
            tracing::warn!(
                "software renderer: transparent surfaces are only supported on Linux/Wayland; this surface will be opaque. Use the hardware backend for transparency on this platform."
            );
        }

        Ok(Self {
            config,
            caches_ready: false,
            // A conservative placeholder until `ensure_caches` reads the real thing off the shaper. Nothing
            // reads it before the first frame, and the first frame runs `ensure_caches` before it draws.
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
            present_history: std::collections::VecDeque::with_capacity(8),
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
    /// Deferred out of the constructors on purpose: the caches are a thread-local and building one loads
    /// fonts, so doing it in `new` would build them on the thread that *made* the renderer rather than the
    /// one that will draw with it. For an on-screen surface those are different threads, and the UI thread's
    /// copy would then sit unused for the life of the process.
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
            // Pre-sized so a `begin_frame` at the same dimensions reuses these buffers instead of reallocating.
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
            present_history: std::collections::VecDeque::with_capacity(8),
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

    // Drains finished background shadow computations into their respective caches. Returns true if at least one shadow became available this frame.
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

    // `op` describes how this frame's pixmap differs from the previous one, used to refresh only the changed part of an aged softbuffer buffer (see plan_present). Pass FrameOp::Full when unsure.
    fn present_pixmap(&mut self, op: FrameOp) -> Result<(), RendererError> {
        let Some(pixmap) = &self.pixmap else {
            return Ok(());
        };
        if self.width == 0 || self.height == 0 {
            return Ok(());
        }
        // Android: copy straight into the ANativeWindow back-buffer. tiny_skia's RGBA byte order matches the native RGBX8888, so presenting is a per-row memcpy with no swizzle.
        #[cfg(target_os = "android")]
        if let Some(nw) = &self.native_window {
            return present_to_native_window(nw, pixmap);
        }

        // Transparent Wayland surface: present the premultiplied-RGBA frame as ARGB8888, keeping alpha (softbuffer can't).
        #[cfg(target_os = "linux")]
        if let Some(alpha) = &mut self.alpha {
            alpha.present(pixmap.data(), self.width, self.height);
            return Ok(());
        }

        // Headless: no surface to blit to; the frame already lives in `self.pixmap`, so presenting is a no-op.
        let Some(surface) = &mut self.surface else {
            return Ok(());
        };

        // Append this frame's change set; an aged buffer is reconstructed by replaying the last `age` entries.
        self.present_history.push_back(op);
        while self.present_history.len() > 6 {
            self.present_history.pop_front();
        }

        let width = self.width as usize;
        let height = self.height as usize;
        if let Ok(mut buffer) = surface.buffer_mut() {
            let age = buffer.age();
            let plan = plan_present(&self.present_history, age);
            // Pixel format: tiny_skia RGBA bytes → softbuffer LE u32 0x00RRGGBB. The damage-aware plan re-swizzles only what changed; a full swizzle of the whole framebuffer is the fallback.
            #[cfg(target_endian = "little")]
            {
                let buf: &mut [u32] = &mut buffer;
                match plan {
                    PresentPlan::Full => convert_rgba_to_xrgb(pixmap.data(), buf),
                    PresentPlan::Regions(regions) => {
                        for r in &regions {
                            convert_rgba_to_xrgb_region(pixmap.data(), buf, width, height, *r);
                        }
                    }
                }
            }
            #[cfg(target_endian = "big")]
            {
                compile_error!(
                    "softbuffer pixel format conversion not implemented for big-endian platforms. \
                              Please file an issue or implement proper endian-aware conversion."
                );
            }
            buffer
                .present()
                .map_err(|e| RendererError::Present(e.to_string()))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use platform_headless::HeadlessWindow;
    use renderer_core::RenderBackend;

    use super::SoftwareRenderer;
    use crate::SoftwareRendererConfig;

    // The caches are a thread-local, so *which* thread builds them decides whether the renderer ever sees its
    // own fonts. Building them in the constructor furnished the UI thread for a renderer that draws on
    // another one — a shaper nobody uses here, and none at all over there. Run on a fresh thread because the
    // test binary's main thread may already have caches from another case.
    #[test]
    fn the_drawing_thread_builds_the_caches_not_the_constructing_one() {
        std::thread::spawn(|| {
            assert!(!crate::caches::initialised(), "a fresh thread starts empty");

            let mut renderer = SoftwareRenderer::<HeadlessWindow, HeadlessWindow>::new_headless(
                8,
                8,
                SoftwareRendererConfig::default(),
            );
            assert!(
                !crate::caches::initialised(),
                "constructing must not load fonts on this thread"
            );

            renderer.begin_frame(8, 8, 1.0, 0).unwrap();
            assert!(
                crate::caches::initialised(),
                "the first frame builds them, on the thread that draws"
            );
        })
        .join()
        .unwrap();
    }
}
