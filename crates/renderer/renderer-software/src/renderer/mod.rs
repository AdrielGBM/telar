mod frame;
mod pixels;
mod present;

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::mpsc;

use clru::{CLruCache, CLruCacheConfig};
use geometry_core::Rect;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use renderer_core::{Color, DrawCommand, RendererError};
use renderer_text::{TextShaper, TextShaperConfig};
use rustc_hash::FxBuildHasher;
use softbuffer::{Context, Surface};
use tiny_skia::Pixmap;

use crate::primitives::image::{ImageCache, PixmapByteScale, ShadowCache};
use crate::primitives::path::{PathShadowCache, new_path_shadow_cache};
use crate::primitives::text::{TextShadowCache, new_text_shadow_cache};

#[cfg(target_endian = "little")]
use pixels::{convert_rgba_to_xrgb, convert_rgba_to_xrgb_region};
#[cfg(target_endian = "little")]
use present::PresentPlan;
use present::{FrameOp, plan_present};
#[cfg(target_os = "android")]
use present::{extract_native_window, present_to_native_window};

pub struct SoftwareRenderer<D: HasDisplayHandle, W: HasWindowHandle> {
    _context: Context<D>,
    surface: Surface<D, W>,
    width: u32,
    height: u32,
    pub(crate) pixmap: Option<Pixmap>,
    pub(crate) text_shaper: TextShaper,
    // Real font ascender/line-height metrics for the default face, queried once at construction so dirty-rect computation does not under-estimate the text region.
    font_metrics: renderer_core::FontMetrics,
    image_cache: ImageCache,
    blur_scratch: Vec<u8>,
    pixmap_pool: Vec<tiny_skia::Pixmap>,
    clip_mask_buffer: Option<tiny_skia::Mask>,
    // Last region written as 0xFF into clip_mask_buffer. Tracked across frames so the next PushClip can zero stale bits left by the previous frame without re-zeroing the whole mask.
    clip_mask_dirty: Option<Rect>,
    draw_state: renderer_core::DrawState,
    shadow_cache: ShadowCache,
    text_pixmap_cache: lru::LruCache<renderer_text::TextCacheKey, tiny_skia::Pixmap>,
    text_shadow_cache: TextShadowCache,
    path_shadow_cache: PathShadowCache,
    // Large shadows are computed on background threads; these maps hold the receivers for in-flight computations keyed by the same cache key, so a frame can poll for completion and avoid re-spawning duplicate work.
    pending_shadows: HashMap<crate::primitives::image::ShadowCacheKey, mpsc::Receiver<Pixmap>>,
    pending_text_shadows:
        HashMap<crate::primitives::text::TextShadowCacheKey, mpsc::Receiver<Pixmap>>,
    pending_path_shadows:
        HashMap<crate::primitives::path::PathShadowCacheKey, mpsc::Receiver<Pixmap>>,
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
        let context = Context::new(display).map_err(|e| {
            RendererError::Backend(format!("softbuffer context creation failed: {}", e))
        })?;
        // Acquire a direct ANativeWindow reference before `window` is moved into softbuffer; used to present without softbuffer's intermediate buffer.
        #[cfg(target_os = "android")]
        let native_window = extract_native_window(&window);
        let surface =
            Surface::new(&context, window).map_err(|e| RendererError::Surface(e.to_string()))?;
        let mut text_shaper = TextShaper::with_config(TextShaperConfig {
            pixel_cache_budget_bytes: config.text_pixel_cache_bytes,
            alpha_cache_budget_bytes: config.text_alpha_cache_bytes,
            shaping_cache_budget_bytes: config.text_shaping_cache_bytes,
            font: config.font,
        });
        let font_metrics = text_shaper.font_metrics();
        Ok(Self {
            _context: context,
            surface,
            width: 0,
            height: 0,
            pixmap: None,
            text_shaper,
            font_metrics,
            image_cache: crate::primitives::image::new_image_cache(config.image_cache_bytes),
            blur_scratch: Vec::new(),
            pixmap_pool: Vec::new(),
            clip_mask_buffer: None,
            clip_mask_dirty: None,
            draw_state: renderer_core::DrawState::new(),
            shadow_cache: CLruCache::with_config(
                CLruCacheConfig::new(NonZeroUsize::new(config.shadow_cache_bytes).unwrap())
                    .with_hasher(FxBuildHasher::default())
                    .with_scale(PixmapByteScale),
            ),
            text_pixmap_cache: lru::LruCache::new(
                std::num::NonZeroUsize::new(config.text_pixmap_cache_entries).unwrap(),
            ),
            text_shadow_cache: new_text_shadow_cache(config.text_shadow_cache_bytes),
            path_shadow_cache: new_path_shadow_cache(config.path_shadow_cache_bytes),
            pending_shadows: HashMap::new(),
            pending_text_shadows: HashMap::new(),
            pending_path_shadows: HashMap::new(),
            layer_stack: Vec::new(),
            prev_commands: Vec::with_capacity(256),
            prev_commands_hash: 0,
            prev_clear_color: None,
            expanded_commands_cache: None,
            layer_bounds_cache: None,
            present_history: std::collections::VecDeque::with_capacity(8),
            #[cfg(target_os = "android")]
            native_window,
        })
    }
    // Drains finished background shadow computations into their respective caches. Returns true if at least one shadow became available this frame.
    fn poll_pending_shadows(&mut self) -> bool {
        let mut arrived = false;
        let shadow_cache = &mut self.shadow_cache;
        self.pending_shadows.retain(|key, rx| match rx.try_recv() {
            Ok(pixmap) => {
                shadow_cache.put_with_weight(key.clone(), pixmap).ok();
                arrived = true;
                false
            }
            Err(mpsc::TryRecvError::Empty) => true,
            Err(mpsc::TryRecvError::Disconnected) => false,
        });
        let text_shadow_cache = &mut self.text_shadow_cache;
        self.pending_text_shadows
            .retain(|key, rx| match rx.try_recv() {
                Ok(pixmap) => {
                    text_shadow_cache.put_with_weight(key.clone(), pixmap).ok();
                    arrived = true;
                    false
                }
                Err(mpsc::TryRecvError::Empty) => true,
                Err(mpsc::TryRecvError::Disconnected) => false,
            });
        let path_shadow_cache = &mut self.path_shadow_cache;
        self.pending_path_shadows
            .retain(|key, rx| match rx.try_recv() {
                Ok(pixmap) => {
                    path_shadow_cache.put_with_weight(key.clone(), pixmap).ok();
                    arrived = true;
                    false
                }
                Err(mpsc::TryRecvError::Empty) => true,
                Err(mpsc::TryRecvError::Disconnected) => false,
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

        // Append this frame's change set; an aged buffer is reconstructed by replaying the last `age` entries.
        self.present_history.push_back(op);
        while self.present_history.len() > 6 {
            self.present_history.pop_front();
        }

        let width = self.width as usize;
        let height = self.height as usize;
        if let Ok(mut buffer) = self.surface.buffer_mut() {
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
