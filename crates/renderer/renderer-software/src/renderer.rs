use std::num::{NonZeroU32, NonZeroUsize};

use clru::{CLruCache, CLruCacheConfig};
use geometry_core::Rect;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use renderer_core::{Color, DrawCommand, RenderBackend, RendererError};
use renderer_text::{TextShaper, TextShaperConfig};
use rustc_hash::FxBuildHasher;
use softbuffer::{Context, Surface};
use tiny_skia::Pixmap;

use crate::primitives::image::{ImageCache, PixmapByteScale, ShadowCache};

fn overlaps_clip(x: f32, y: f32, w: f32, h: f32, clip: Option<Rect>) -> bool {
    let Some(clip) = clip else { return true };
    x + w > clip.x && y + h > clip.y && x < clip.x + clip.width && y < clip.y + clip.height
}

fn path_data_bounds(data: &renderer_core::PathData) -> Option<(f32, f32, f32, f32)> {
    use renderer_core::PathVerb;
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut include = |x: f32, y: f32| {
        if x < min_x {
            min_x = x;
        }
        if y < min_y {
            min_y = y;
        }
        if x > max_x {
            max_x = x;
        }
        if y > max_y {
            max_y = y;
        }
    };
    for v in data.verbs() {
        match v {
            PathVerb::MoveTo(p) | PathVerb::LineTo(p) => include(p.x, p.y),
            PathVerb::QuadTo { ctrl, to } => {
                include(ctrl.x, ctrl.y);
                include(to.x, to.y);
            }
            PathVerb::CubicTo { ctrl1, ctrl2, to } => {
                include(ctrl1.x, ctrl1.y);
                include(ctrl2.x, ctrl2.y);
                include(to.x, to.y);
            }
            PathVerb::Close => {}
        }
    }
    if min_x.is_finite() && min_y.is_finite() {
        Some((min_x, min_y, max_x - min_x, max_y - min_y))
    } else {
        None
    }
}

fn clamp_to_pixels(rect: Rect, width: u32, height: u32) -> Option<(u32, u32, u32, u32)> {
    let x0 = rect.x.floor().max(0.0) as i64;
    let y0 = rect.y.floor().max(0.0) as i64;
    let x1 = (rect.x + rect.width).ceil().max(0.0) as i64;
    let y1 = (rect.y + rect.height).ceil().max(0.0) as i64;
    let x0 = x0.min(width as i64) as u32;
    let y0 = y0.min(height as i64) as u32;
    let x1 = x1.min(width as i64) as u32;
    let y1 = y1.min(height as i64) as u32;
    if x1 > x0 && y1 > y0 {
        Some((x0, y0, x1, y1))
    } else {
        None
    }
}

fn fill_mask_region(data: &mut [u8], stride: usize, region: (u32, u32, u32, u32), value: u8) {
    let (x0, y0, x1, y1) = region;
    let row_len = (x1 - x0) as usize;
    for y in y0..y1 {
        let start = y as usize * stride + x0 as usize;
        data[start..start + row_len].fill(value);
    }
}

// Updates the 1-bit clip mask in place. Only touches rows/cols within the union of the previous and new clip rects, avoiding the full-buffer zero (~2MB at 1080p) that would otherwise run on every PushClip/PopClip. Writes 0xFF directly because clip rects are axis-aligned and the existing fill_path used anti_alias=false (binary mask).
fn repaint_mask(
    mask: &mut tiny_skia::Mask,
    new_rect: Rect,
    prev_rect: Option<Rect>,
    width: u32,
    height: u32,
) {
    let stride = width as usize;
    let data = mask.data_mut();
    if let Some(prev) = prev_rect {
        if let Some(region) = clamp_to_pixels(prev, width, height) {
            fill_mask_region(data, stride, region, 0);
        }
    }
    if let Some(region) = clamp_to_pixels(new_rect, width, height) {
        fill_mask_region(data, stride, region, 0xFF);
    }
}

pub struct SoftwareRenderer<D: HasDisplayHandle, W: HasWindowHandle> {
    _context: Context<D>,
    surface: Surface<D, W>,
    width: u32,
    height: u32,
    pub(crate) pixmap: Option<Pixmap>,
    pub(crate) text_shaper: TextShaper,
    image_cache: ImageCache,
    blur_scratch: Vec<u8>,
    pixmap_pool: Vec<tiny_skia::Pixmap>,
    clip_mask_buf: Option<tiny_skia::Mask>,
    // Last region written as 0xFF into clip_mask_buf. Tracked across frames so the next PushClip can zero stale bits left by the previous frame without re-zeroing the whole mask.
    clip_mask_dirty: Option<Rect>,
    draw_state: renderer_core::DrawState,
    shadow_cache: ShadowCache,
    text_pixmap_cache: lru::LruCache<renderer_text::TextCacheKey, tiny_skia::Pixmap>,
    layer_stack: Vec<(tiny_skia::Pixmap, f32)>,
}

impl<D, W> SoftwareRenderer<D, W>
where
    D: HasDisplayHandle,
    W: HasWindowHandle,
{
    pub fn new(
        display: D,
        window: W,
        budget: crate::RendererBudget,
    ) -> Result<Self, RendererError> {
        let context = Context::new(display).map_err(|e| {
            RendererError::Backend(format!("softbuffer context creation failed: {}", e))
        })?;
        let surface =
            Surface::new(&context, window).map_err(|e| RendererError::Surface(e.to_string()))?;
        Ok(Self {
            _context: context,
            surface,
            width: 0,
            height: 0,
            pixmap: None,
            text_shaper: TextShaper::with_config(TextShaperConfig {
                pixel_cache_budget_bytes: budget.text_pixel_cache_bytes,
                alpha_cache_budget_bytes: budget.text_alpha_cache_bytes,
                shaping_cache_budget_bytes: budget.text_shaping_cache_bytes,
            }),
            image_cache: crate::primitives::image::new_image_cache(budget.image_cache_bytes),
            blur_scratch: Vec::new(),
            pixmap_pool: Vec::new(),
            clip_mask_buf: None,
            clip_mask_dirty: None,
            draw_state: renderer_core::DrawState::new(),
            shadow_cache: CLruCache::with_config(
                CLruCacheConfig::new(NonZeroUsize::new(budget.shadow_cache_bytes).unwrap())
                    .with_hasher(FxBuildHasher::default())
                    .with_scale(PixmapByteScale),
            ),
            text_pixmap_cache: lru::LruCache::new(
                std::num::NonZeroUsize::new(budget.text_pixmap_cache_entries).unwrap(),
            ),
            layer_stack: Vec::new(),
        })
    }
}

impl<D, W> RenderBackend for SoftwareRenderer<D, W>
where
    D: HasDisplayHandle,
    W: HasWindowHandle,
{
    fn begin_frame(&mut self, width: u32, height: u32) -> Result<(), RendererError> {
        if width != self.width || height != self.height {
            self.width = width;
            self.height = height;
            self.pixmap = Pixmap::new(width, height);
            self.clip_mask_buf = tiny_skia::Mask::new(width, height);
            self.clip_mask_dirty = None;
            self.pixmap_pool.clear();
            if let (Some(w), Some(h)) = (NonZeroU32::new(width), NonZeroU32::new(height)) {
                self.surface
                    .resize(w, h)
                    .map_err(|e| RendererError::Resize(e.to_string()))?;
            }
        }
        Ok(())
    }

    fn render_frame(
        &mut self,
        commands: &[DrawCommand],
        clear_color: Option<Color>,
    ) -> Result<(), RendererError> {
        if let (Some(color), Some(pixmap)) = (clear_color, &mut self.pixmap) {
            pixmap.fill(crate::primitives::to_skia_color(color));
        }

        self.draw_state.reset();
        let mut clip_active: bool = false;
        let mut current_clip_rect: Option<Rect> = None;
        self.layer_stack.clear();

        for cmd in commands {
            if self.pixmap.is_none() {
                break;
            }
            let transform = tiny_skia::Transform::from_translate(
                self.draw_state.cum_tx,
                self.draw_state.cum_ty,
            );
            match cmd {
                DrawCommand::Rect(p) => {
                    if p.rect.width <= 0.0
                        || p.rect.height <= 0.0
                        || (p.style.fill.is_none() && p.style.stroke.is_none())
                    {
                        continue;
                    }
                    if !overlaps_clip(
                        p.rect.x,
                        p.rect.y,
                        p.rect.width,
                        p.rect.height,
                        current_clip_rect,
                    ) {
                        continue;
                    }
                    let pixmap = if let Some((top, _)) = self.layer_stack.last_mut() {
                        top
                    } else {
                        self.pixmap.as_mut().unwrap()
                    };
                    let clip = if clip_active {
                        self.clip_mask_buf.as_ref()
                    } else {
                        None
                    };
                    crate::primitives::rect::draw_rect(
                        pixmap,
                        p.rect,
                        &p.style,
                        transform,
                        clip,
                        current_clip_rect,
                        &mut self.shadow_cache,
                        &mut self.blur_scratch,
                    );
                }
                DrawCommand::Text(p) => {
                    if !overlaps_clip(
                        p.rect.x,
                        p.rect.y,
                        p.rect.width,
                        p.rect.height,
                        current_clip_rect,
                    ) {
                        continue;
                    }
                    let pixmap = if let Some((top, _)) = self.layer_stack.last_mut() {
                        top
                    } else {
                        self.pixmap.as_mut().unwrap()
                    };
                    let clip = if clip_active {
                        self.clip_mask_buf.as_ref()
                    } else {
                        None
                    };
                    crate::primitives::text::draw_text(
                        pixmap,
                        &mut self.text_shaper,
                        &p.text,
                        p.rect,
                        &p.style,
                        transform,
                        clip,
                        current_clip_rect,
                        &mut self.blur_scratch,
                        &mut self.text_pixmap_cache,
                    );
                }
                DrawCommand::Image { data, rect, filter } => {
                    if !overlaps_clip(rect.x, rect.y, rect.width, rect.height, current_clip_rect) {
                        continue;
                    }
                    let pixmap = if let Some((top, _)) = self.layer_stack.last_mut() {
                        top
                    } else {
                        self.pixmap.as_mut().unwrap()
                    };
                    let clip = if clip_active {
                        self.clip_mask_buf.as_ref()
                    } else {
                        None
                    };
                    crate::primitives::image::draw_image(
                        pixmap,
                        data,
                        &mut self.image_cache,
                        *rect,
                        *filter,
                        transform,
                        clip,
                    );
                }
                DrawCommand::Line { p1, p2, style } => {
                    let min_x = p1.x.min(p2.x);
                    let min_y = p1.y.min(p2.y);
                    let w = (p1.x.max(p2.x) - min_x).max(0.0);
                    let h = (p1.y.max(p2.y) - min_y).max(0.0);
                    if !overlaps_clip(min_x, min_y, w, h, current_clip_rect) {
                        continue;
                    }
                    let pixmap = if let Some((top, _)) = self.layer_stack.last_mut() {
                        top
                    } else {
                        self.pixmap.as_mut().unwrap()
                    };
                    let clip = if clip_active {
                        self.clip_mask_buf.as_ref()
                    } else {
                        None
                    };
                    crate::primitives::line::draw_line(pixmap, *p1, *p2, *style, transform, clip);
                }
                DrawCommand::Path(p) => {
                    if let Some((bx, by, bw, bh)) = path_data_bounds(&p.data) {
                        if !overlaps_clip(bx, by, bw, bh, current_clip_rect) {
                            continue;
                        }
                    }
                    let pixmap = if let Some((top, _)) = self.layer_stack.last_mut() {
                        top
                    } else {
                        self.pixmap.as_mut().unwrap()
                    };
                    let clip = if clip_active {
                        self.clip_mask_buf.as_ref()
                    } else {
                        None
                    };
                    crate::primitives::path::draw_path(
                        pixmap,
                        &p.data,
                        &p.style,
                        transform,
                        clip,
                        current_clip_rect,
                        &mut self.blur_scratch,
                    );
                }
                DrawCommand::PushClip { rect } => {
                    let prev_dirty = self.clip_mask_dirty;
                    let effective = self.draw_state.push_clip(*rect);
                    current_clip_rect = Some(effective);
                    if let Some(ref mut m) = self.clip_mask_buf {
                        repaint_mask(m, effective, prev_dirty, self.width, self.height);
                    }
                    self.clip_mask_dirty = Some(effective);
                    clip_active = true;
                }
                DrawCommand::PopClip => {
                    let prev_dirty = self.clip_mask_dirty;
                    let effective = self.draw_state.pop_clip();
                    match effective {
                        Some(r) => {
                            current_clip_rect = Some(r);
                            if let Some(ref mut m) = self.clip_mask_buf {
                                repaint_mask(m, r, prev_dirty, self.width, self.height);
                            }
                            self.clip_mask_dirty = Some(r);
                            clip_active = true;
                        }
                        None => {
                            if let (Some(ref mut m), Some(prev_rect)) =
                                (self.clip_mask_buf.as_mut(), prev_dirty)
                            {
                                if let Some(region) =
                                    clamp_to_pixels(prev_rect, self.width, self.height)
                                {
                                    fill_mask_region(m.data_mut(), self.width as usize, region, 0);
                                }
                            }
                            self.clip_mask_dirty = None;
                            current_clip_rect = None;
                            clip_active = false;
                        }
                    }
                }
                DrawCommand::PushTransform { tx, ty } => {
                    self.draw_state.push_transform(*tx, *ty);
                }
                DrawCommand::PopTransform => {
                    self.draw_state.pop_transform();
                }
                DrawCommand::PushLayer { opacity } => {
                    let layer = self
                        .pixmap_pool
                        .pop()
                        .filter(|p| p.width() == self.width && p.height() == self.height)
                        .or_else(|| tiny_skia::Pixmap::new(self.width, self.height));
                    if let Some(mut l) = layer {
                        l.fill(tiny_skia::Color::TRANSPARENT);
                        self.layer_stack.push((l, *opacity));
                    }
                }
                DrawCommand::PopLayer => {
                    if let Some((layer, opacity)) = self.layer_stack.pop() {
                        let target = if let Some((top, _)) = self.layer_stack.last_mut() {
                            top
                        } else {
                            self.pixmap.as_mut().unwrap()
                        };
                        target.draw_pixmap(
                            0,
                            0,
                            layer.as_ref(),
                            &tiny_skia::PixmapPaint {
                                opacity,
                                blend_mode: tiny_skia::BlendMode::SourceOver,
                                quality: tiny_skia::FilterQuality::Nearest,
                            },
                            tiny_skia::Transform::identity(),
                            None,
                        );
                        self.pixmap_pool.push(layer);
                    }
                }
            }
        }

        let Some(pixmap) = &self.pixmap else {
            return Ok(());
        };
        if self.width == 0 || self.height == 0 {
            return Ok(());
        }
        if let Ok(mut buffer) = self.surface.buffer_mut() {
            // SAFETY: Pixel format conversion. tiny-skia stores pixels as RGBA in little-endian byte order: [R, G, B, A, ...]. softbuffer::Buffer accepts u32 pixels in platform-native endianness. On little-endian platforms: u32(0x00RRGGBB) → bytes [B, G, R, 0] in memory → correct. On big-endian platforms: u32(0x00RRGGBB) → bytes [0, R, G, B] in memory → incorrect. This code is only correct on little-endian.
            #[cfg(target_endian = "little")]
            {
                let src: &[u32] = bytemuck::cast_slice(pixmap.data());
                for (dst, &src_px) in buffer.iter_mut().zip(src.iter()) {
                    *dst = (src_px >> 8) & 0x00FF_FFFF;
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
