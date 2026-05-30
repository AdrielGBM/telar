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
use crate::primitives::path::{PathShadowCache, new_path_shadow_cache};
use crate::primitives::text::{TextShadowCache, new_text_shadow_cache};

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

fn rect_overlaps(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.width && a.x + a.width > b.x && a.y < b.y + b.height && a.y + a.height > b.y
}

fn union_opt_rect(acc: Option<Rect>, r: Rect) -> Option<Rect> {
    Some(match acc {
        None => r,
        Some(a) => {
            let x = a.x.min(r.x);
            let y = a.y.min(r.y);
            let x2 = (a.x + a.width).max(r.x + r.width);
            let y2 = (a.y + a.height).max(r.y + r.height);
            Rect {
                x,
                y,
                width: x2 - x,
                height: y2 - y,
            }
        }
    })
}

// Returns Some(fill_alpha) when the rect should be rendered via an intermediate layer to avoid
// the AA-fringe artifact that occurs when geometric coverage × fill_alpha is less than fill_alpha
// at the edges of a rounded rect, making the border bleed more than the interior.
fn fill_layer_alpha(style: &renderer_core::RectStyle) -> Option<f32> {
    // Skip when shadow is present: shadow.color.a controls shadow opacity independently and would
    // be incorrectly scaled inside a fill-alpha layer.
    if style.radius.is_zero() || style.shadow.is_some() {
        return None;
    }
    match style.fill {
        Some(renderer_core::FillStyle::Solid(c)) if c.a > 0.0 && c.a < 1.0 => Some(c.a),
        _ => None,
    }
}

// Expands each semi-transparent solid-fill rounded rect into PushLayer{opacity} + opaque Rect +
// PopLayer. This separates geometric AA coverage from fill transparency so the renderer composites
// them correctly: the layer captures the fully-opaque shape (correct AA edge), then composites the
// whole layer at fill_alpha, avoiding the visible fringe on high-contrast backgrounds.
fn expand_fill_layers(commands: &[DrawCommand]) -> Option<Vec<DrawCommand>> {
    if !commands
        .iter()
        .any(|cmd| matches!(cmd, DrawCommand::Rect(p) if fill_layer_alpha(&p.style).is_some()))
    {
        return None;
    }
    let mut result = Vec::with_capacity(commands.len() + 4);
    for cmd in commands {
        if let DrawCommand::Rect(p) = cmd {
            if let Some(alpha) = fill_layer_alpha(&p.style) {
                let mut opaque = (**p).clone();
                if let Some(renderer_core::FillStyle::Solid(c)) = opaque.style.fill {
                    opaque.style.fill =
                        Some(renderer_core::FillStyle::Solid(renderer_core::Color {
                            a: 1.0,
                            ..c
                        }));
                }
                result.push(DrawCommand::PushLayer {
                    opacity: alpha,
                    backdrop_blur: 0.0,
                    clip_radius: 0.0,
                });
                result.push(DrawCommand::Rect(Box::new(opaque)));
                result.push(DrawCommand::PopLayer);
                continue;
            }
        }
        result.push(cmd.clone());
    }
    Some(result)
}

fn compute_layer_bboxes(
    commands: &[DrawCommand],
    window_w: u32,
    window_h: u32,
) -> Vec<Option<(i32, i32, u32, u32)>> {
    let mut result = vec![None; commands.len()];
    let mut stack: Vec<(usize, Option<Rect>)> = Vec::new();
    let mut cum_matrix = renderer_core::IDENTITY_MATRIX;
    let mut matrix_stack: Vec<[f32; 6]> = Vec::new();

    for (idx, cmd) in commands.iter().enumerate() {
        match cmd {
            DrawCommand::PushMatrix { matrix } => {
                matrix_stack.push(cum_matrix);
                cum_matrix = renderer_core::compose_matrix(cum_matrix, *matrix);
            }
            DrawCommand::PopMatrix => {
                if let Some(prev) = matrix_stack.pop() {
                    cum_matrix = prev;
                }
            }
            DrawCommand::PushLayer { .. } => {
                stack.push((idx, None));
            }
            DrawCommand::PopLayer => {
                if let Some((push_idx, accumulated)) = stack.pop() {
                    let (ox, oy, bw, bh) = if let Some(bbox) = accumulated {
                        let x0 = bbox.x.floor().max(0.0).min(window_w as f32) as i32;
                        let y0 = bbox.y.floor().max(0.0).min(window_h as f32) as i32;
                        let x1 = (bbox.x + bbox.width).ceil().max(0.0).min(window_w as f32) as i32;
                        let y1 = (bbox.y + bbox.height).ceil().max(0.0).min(window_h as f32) as i32;
                        let w = (x1 - x0).max(1) as u32;
                        let h = (y1 - y0).max(1) as u32;
                        (x0, y0, w, h)
                    } else {
                        (0, 0, window_w, window_h)
                    };
                    result[push_idx] = Some((ox, oy, bw, bh));
                }
            }
            _ => {
                if let Some(vr) = renderer_core::culling::command_visual_rect(cmd, cum_matrix) {
                    if !stack.is_empty() {
                        let last_idx = stack.len() - 1;
                        stack[last_idx].1 = union_opt_rect(stack[last_idx].1, vr);
                    }
                }
            }
        }
    }

    result
}

// Shifts rows (Y scroll) or columns (X scroll) inside `clip` in place; the two are mutually exclusive. The newly exposed strip is left stale and must be re-rendered by the caller.
fn apply_scroll_blit(pixmap: &mut Pixmap, clip: Rect, delta_tx: f32, delta_ty: f32) {
    let width = pixmap.width() as usize;
    let height = pixmap.height() as usize;
    let x0 = (clip.x.floor() as usize).min(width);
    let y0 = (clip.y.floor() as usize).min(height);
    let x1 = ((clip.x + clip.width).ceil() as usize).min(width);
    let y1 = ((clip.y + clip.height).ceil() as usize).min(height);
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let data = pixmap.data_mut();
    let dy = delta_ty.round() as i64;
    let dx = delta_tx.round() as i64;
    if dy != 0 {
        let row_bytes = (x1 - x0) * 4;
        if dy < 0 {
            // Content moved up: write to a lower row, read from a higher row → top-to-bottom is safe.
            let shift = (-dy) as usize;
            for dst_y in y0..y1 {
                let src_y = dst_y + shift;
                if src_y >= y1 {
                    break;
                }
                let src_off = (src_y * width + x0) * 4;
                let dst_off = (dst_y * width + x0) * 4;
                data.copy_within(src_off..src_off + row_bytes, dst_off);
            }
        } else {
            // Content moved down: write to a higher row, read from a lower row → bottom-to-top is safe.
            let shift = dy as usize;
            for dst_y in (y0..y1).rev() {
                if dst_y < y0 + shift {
                    break;
                }
                let src_y = dst_y - shift;
                if src_y < y0 {
                    break;
                }
                let src_off = (src_y * width + x0) * 4;
                let dst_off = (dst_y * width + x0) * 4;
                data.copy_within(src_off..src_off + row_bytes, dst_off);
            }
        }
    } else if dx != 0 {
        if dx < 0 {
            // Content moved left: read from a higher column, write to a lower column → left-to-right is safe.
            let shift = (-dx) as usize;
            for y in y0..y1 {
                let row_base = y * width;
                for dst_x in x0..x1 {
                    let src_x = dst_x + shift;
                    if src_x >= x1 {
                        break;
                    }
                    let src_off = (row_base + src_x) * 4;
                    let dst_off = (row_base + dst_x) * 4;
                    data.copy_within(src_off..src_off + 4, dst_off);
                }
            }
        } else {
            // Content moved right: read from a lower column, write to a higher column → right-to-left is safe.
            let shift = dx as usize;
            for y in y0..y1 {
                let row_base = y * width;
                for dst_x in (x0..x1).rev() {
                    if dst_x < x0 + shift {
                        break;
                    }
                    let src_x = dst_x - shift;
                    let src_off = (row_base + src_x) * 4;
                    let dst_off = (row_base + dst_x) * 4;
                    data.copy_within(src_off..src_off + 4, dst_off);
                }
            }
        }
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
    if prev_rect == Some(new_rect) {
        return;
    }
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
    text_shadow_cache: TextShadowCache,
    path_shadow_cache: PathShadowCache,
    layer_stack: Vec<(tiny_skia::Pixmap, f32, i32, i32, f32)>,
    // Previous frame state for skip-if-identical and dirty-rect optimizations.
    prev_commands: Vec<DrawCommand>,
    prev_clear_color: Option<Color>,
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
            text_shadow_cache: new_text_shadow_cache(budget.text_shadow_cache_bytes),
            path_shadow_cache: new_path_shadow_cache(budget.path_shadow_cache_bytes),
            layer_stack: Vec::new(),
            prev_commands: Vec::with_capacity(256),
            prev_clear_color: None,
        })
    }
    fn present_pixmap(&mut self) -> Result<(), RendererError> {
        let Some(pixmap) = &self.pixmap else {
            return Ok(());
        };
        if self.width == 0 || self.height == 0 {
            return Ok(());
        }
        if let Ok(mut buffer) = self.surface.buffer_mut() {
            // Pixel format conversion: tiny-skia stores pixels as premultiplied RGBA bytes [R, G, B, A, ...]. softbuffer expects u32 pixels as 0x00RRGGBB in native endianness. On little-endian, the bytemuck cast gives 0xAABBGGRR per pixel; swap_bytes() reorders to 0xRRGGBBAA and >> 8 drops the alpha byte to yield 0x00RRGGBB.
            #[cfg(target_endian = "little")]
            {
                let src: &[u32] = bytemuck::cast_slice(pixmap.data());
                for (dst, &src_px) in buffer.iter_mut().zip(src.iter()) {
                    *dst = src_px.swap_bytes() >> 8;
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
            self.prev_commands.clear();
            self.prev_clear_color = None;
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
        // Optimization 1: skip the entire render when nothing changed; just re-present the existing pixmap.
        if commands == self.prev_commands.as_slice() && clear_color == self.prev_clear_color {
            return self.present_pixmap();
        }

        // Optimization 2: scroll blit. When the only change is a single PushTransform ty-shift (a scroll event), shift the existing pixel rows in place and only re-render the exposed band plus any out-of-clip overlays that changed (e.g. the scrollbar).
        let maybe_scroll = if !self.prev_commands.is_empty() {
            renderer_core::dirty::detect_scroll_blit(commands, &self.prev_commands)
        } else {
            None
        };
        if let Some(ref sb) = maybe_scroll {
            if let Some(pixmap) = &mut self.pixmap {
                apply_scroll_blit(
                    pixmap,
                    sb.scroll_clip,
                    sb.delta_tx as f32,
                    sb.delta_ty as f32,
                );
            }
        }

        // Optimization 3: compute the on-screen union of all changed commands so we can clear only that region.
        let dirty_rect = if let Some(ref sb) = maybe_scroll {
            // Scroll blit case: only re-render the newly exposed band and any changed overlays.
            let base = sb.exposed_band;
            Some(match sb.extra_dirty {
                Some(ed) => union_opt_rect(Some(base), ed).unwrap(),
                None => base,
            })
        } else if self.prev_commands.is_empty() {
            None // first frame → full clear
        } else {
            renderer_core::dirty::compute_dirty_rect(
                commands,
                &self.prev_commands,
                renderer_core::culling::command_visual_rect,
            )
        };

        let clear_color_changed = clear_color != self.prev_clear_color;
        self.prev_commands.clear();
        self.prev_commands.extend(commands.iter().cloned());
        self.prev_clear_color = clear_color;

        // Clear either the dirty region only or the full pixmap when a structural change forces a full re-render; IMPORTANT: compute both the tiny-skia clear rect and the geometry rect used for command-skipping from the same clamped bounds because the naive (dr.x-1).max(0) / dr.width+2 formula shifts the rect right/down when dr has negative coordinates (off-screen content), so fill_rect would clear a larger on-screen area than `dr` describes — causing commands outside `dr` to have their pixels cleared and then be skipped, which makes them disappear.
        let skip_rect: Option<Rect> = match dirty_rect {
            Some(dr) if dr.width > 0.0 && dr.height > 0.0 => {
                let x0 = (dr.x - 1.0).max(0.0);
                let y0 = (dr.y - 1.0).max(0.0);
                let x1 = (dr.x + dr.width + 1.0).min(self.width as f32);
                let y1 = (dr.y + dr.height + 1.0).min(self.height as f32);
                if x1 > x0 && y1 > y0 {
                    // Expand skip_rect to fully contain every command it partially intersects: a partially-overlapping command is still fully redrawn, overwriting pixels of earlier commands that fall outside the region and won't be redrawn themselves.
                    let mut sr = Rect {
                        x: x0,
                        y: y0,
                        width: x1 - x0,
                        height: y1 - y0,
                    };
                    let mut sr_matrix = renderer_core::IDENTITY_MATRIX;
                    let mut sr_matrix_stk: Vec<[f32; 6]> = Vec::new();
                    for cmd in commands.iter() {
                        match cmd {
                            DrawCommand::PushMatrix { matrix } => {
                                sr_matrix_stk.push(sr_matrix);
                                sr_matrix = renderer_core::compose_matrix(sr_matrix, *matrix);
                            }
                            DrawCommand::PopMatrix => {
                                if let Some(prev) = sr_matrix_stk.pop() {
                                    sr_matrix = prev;
                                }
                            }
                            _ => {
                                if let Some(vr) =
                                    renderer_core::culling::command_visual_rect(cmd, sr_matrix)
                                {
                                    if rect_overlaps(vr, sr) {
                                        let nx = sr.x.min(vr.x);
                                        let ny = sr.y.min(vr.y);
                                        let nx2 = (sr.x + sr.width).max(vr.x + vr.width);
                                        let ny2 = (sr.y + sr.height).max(vr.y + vr.height);
                                        sr = Rect {
                                            x: nx,
                                            y: ny,
                                            width: nx2 - nx,
                                            height: ny2 - ny,
                                        };
                                    }
                                }
                            }
                        }
                    }
                    // Re-clamp to viewport after expansion.
                    let fx0 = sr.x.max(0.0);
                    let fy0 = sr.y.max(0.0);
                    let fx1 = (sr.x + sr.width).min(self.width as f32);
                    let fy1 = (sr.y + sr.height).min(self.height as f32);
                    if fx1 > fx0 && fy1 > fy0 {
                        Some(Rect {
                            x: fx0,
                            y: fy0,
                            width: fx1 - fx0,
                            height: fy1 - fy0,
                        })
                    } else {
                        return self.present_pixmap();
                    }
                } else {
                    // Dirty region is entirely off-screen — nothing visible changed.
                    return self.present_pixmap();
                }
            }
            _ => None,
        };

        // If the clear color changed, the dirty-rect only covers command-changed regions, leaving
        // background areas untouched with stale pixels from the previous frame. Force a full clear.
        let skip_rect = if clear_color_changed { None } else { skip_rect };

        if let (Some(color), Some(pixmap)) = (clear_color, &mut self.pixmap) {
            if let Some(sr) = skip_rect {
                let skia_rect = tiny_skia::Rect::from_xywh(sr.x, sr.y, sr.width, sr.height);
                if let Some(r) = skia_rect {
                    let mut paint = tiny_skia::Paint::default();
                    paint.set_color(crate::primitives::to_skia_color(color));
                    paint.blend_mode = tiny_skia::BlendMode::Source;
                    pixmap.fill_rect(r, &paint, tiny_skia::Transform::identity(), None);
                } else {
                    pixmap.fill(crate::primitives::to_skia_color(color));
                }
            } else {
                pixmap.fill(crate::primitives::to_skia_color(color));
            }
        }

        self.draw_state.reset();
        let mut clip_active: bool = false;
        let mut current_clip_rect: Option<Rect> = None;
        self.layer_stack.clear();

        let expanded_commands = expand_fill_layers(commands);
        let commands: &[DrawCommand] = expanded_commands.as_deref().unwrap_or(commands);

        let layer_bboxes = compute_layer_bboxes(commands, self.width, self.height);

        for (cmd_idx, cmd) in commands.iter().enumerate() {
            if self.pixmap.is_none() {
                break;
            }

            let inside_layer = !self.layer_stack.is_empty();
            let (layer_ox, layer_oy) = self
                .layer_stack
                .last()
                .map(|(_, _, ox, oy, _)| (*ox, *oy))
                .unwrap_or((0, 0));

            let [ma, mb, mc, md, me, mf] = self.draw_state.cum_matrix;
            let transform = tiny_skia::Transform::from_row(
                ma,
                mb,
                mc,
                md,
                me - layer_ox as f32,
                mf - layer_oy as f32,
            );

            // Optimization 3: skip draw commands whose visual bounds don't overlap the dirty region, use skip_rect (the actual clamped on-screen clear bounds) so the skip check is consistent with what fill_rect actually cleared, and always execute state commands (PushTransform, PushClip, PushLayer, etc.) because they return None.
            if let Some(sr) = skip_rect {
                if let Some(vr) =
                    renderer_core::culling::command_visual_rect(cmd, self.draw_state.cum_matrix)
                {
                    if !rect_overlaps(vr, sr) {
                        continue;
                    }
                }
            }

            match cmd {
                DrawCommand::Rect(p) => {
                    if p.rect.width <= 0.0
                        || p.rect.height <= 0.0
                        || (p.style.fill.is_none() && p.style.stroke.is_none())
                    {
                        continue;
                    }
                    let (spr_x, spr_y) = self.draw_state.apply_point(p.rect.x, p.rect.y);
                    let (spr_x2, spr_y2) = self
                        .draw_state
                        .apply_point(p.rect.x + p.rect.width, p.rect.y + p.rect.height);
                    if !renderer_core::culling::overlaps(
                        spr_x.min(spr_x2),
                        spr_y.min(spr_y2),
                        (spr_x2 - spr_x).abs(),
                        (spr_y2 - spr_y).abs(),
                        current_clip_rect,
                    ) {
                        continue;
                    }
                    let pixmap = if let Some((layer, _, _, _, _)) = self.layer_stack.last_mut() {
                        layer
                    } else {
                        self.pixmap.as_mut().unwrap()
                    };
                    let clip = if clip_active && !inside_layer {
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
                        &mut self.shadow_cache,
                        &mut self.blur_scratch,
                    );
                }
                DrawCommand::Text(p) => {
                    let (spt_x, spt_y) = self.draw_state.apply_point(p.rect.x, p.rect.y);
                    let (spt_x2, spt_y2) = self
                        .draw_state
                        .apply_point(p.rect.x + p.rect.width, p.rect.y + p.rect.height);
                    if !renderer_core::culling::overlaps(
                        spt_x.min(spt_x2),
                        spt_y.min(spt_y2),
                        (spt_x2 - spt_x).abs(),
                        (spt_y2 - spt_y).abs(),
                        current_clip_rect,
                    ) {
                        continue;
                    }
                    let pixmap = if let Some((top, _, _, _, _)) = self.layer_stack.last_mut() {
                        top
                    } else {
                        self.pixmap.as_mut().unwrap()
                    };
                    let clip = if clip_active && !inside_layer {
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
                        &mut self.text_shadow_cache,
                    );
                }
                DrawCommand::Image { data, rect, filter } => {
                    let (spi_x, spi_y) = self.draw_state.apply_point(rect.x, rect.y);
                    let (spi_x2, spi_y2) = self
                        .draw_state
                        .apply_point(rect.x + rect.width, rect.y + rect.height);
                    if !renderer_core::culling::overlaps(
                        spi_x.min(spi_x2),
                        spi_y.min(spi_y2),
                        (spi_x2 - spi_x).abs(),
                        (spi_y2 - spi_y).abs(),
                        current_clip_rect,
                    ) {
                        continue;
                    }
                    let pixmap = if let Some((top, _, _, _, _)) = self.layer_stack.last_mut() {
                        top
                    } else {
                        self.pixmap.as_mut().unwrap()
                    };
                    let clip = if clip_active && !inside_layer {
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
                    let (lp1x, lp1y) = self.draw_state.apply_point(p1.x, p1.y);
                    let (lp2x, lp2y) = self.draw_state.apply_point(p2.x, p2.y);
                    let min_x = lp1x.min(lp2x);
                    let min_y = lp1y.min(lp2y);
                    let w = (lp1x.max(lp2x) - min_x).max(0.0);
                    let h = (lp1y.max(lp2y) - min_y).max(0.0);
                    if !renderer_core::culling::overlaps(min_x, min_y, w, h, current_clip_rect) {
                        continue;
                    }
                    let pixmap = if let Some((top, _, _, _, _)) = self.layer_stack.last_mut() {
                        top
                    } else {
                        self.pixmap.as_mut().unwrap()
                    };
                    let clip = if clip_active && !inside_layer {
                        self.clip_mask_buf.as_ref()
                    } else {
                        None
                    };
                    crate::primitives::line::draw_line(
                        pixmap,
                        *p1,
                        *p2,
                        *style,
                        transform,
                        clip,
                        current_clip_rect,
                    );
                }
                DrawCommand::Path(p) => {
                    if let Some(b) = p.data.bounds() {
                        let (sp_x, sp_y) = self.draw_state.apply_point(b.x, b.y);
                        let (sp_x2, sp_y2) =
                            self.draw_state.apply_point(b.x + b.width, b.y + b.height);
                        if !renderer_core::culling::overlaps(
                            sp_x.min(sp_x2),
                            sp_y.min(sp_y2),
                            (sp_x2 - sp_x).abs(),
                            (sp_y2 - sp_y).abs(),
                            current_clip_rect,
                        ) {
                            continue;
                        }
                    }
                    let pixmap = if let Some((top, _, _, _, _)) = self.layer_stack.last_mut() {
                        top
                    } else {
                        self.pixmap.as_mut().unwrap()
                    };
                    let clip = if clip_active && !inside_layer {
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
                        &mut self.path_shadow_cache,
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
                DrawCommand::PushMatrix { matrix } => {
                    self.draw_state.push_matrix(*matrix);
                }
                DrawCommand::PopMatrix => {
                    self.draw_state.pop_matrix();
                }
                DrawCommand::PushLayer {
                    opacity,
                    backdrop_blur,
                    clip_radius,
                } => {
                    let (ox, oy, bw, bh) =
                        layer_bboxes[cmd_idx].unwrap_or((0, 0, self.width, self.height));
                    let layer = self
                        .pixmap_pool
                        .pop()
                        .filter(|p| p.width() == bw && p.height() == bh)
                        .or_else(|| tiny_skia::Pixmap::new(bw, bh));
                    if let Some(mut l) = layer {
                        if *backdrop_blur > 0.0 {
                            let (pox, poy) = self
                                .layer_stack
                                .last()
                                .map(|(_, _, pox, poy, _)| (*pox, *poy))
                                .unwrap_or((0, 0));
                            let parent = if let Some((top, _, _, _, _)) = self.layer_stack.last() {
                                top
                            } else {
                                self.pixmap.as_ref().unwrap()
                            };
                            l.fill(tiny_skia::Color::TRANSPARENT);
                            l.draw_pixmap(
                                pox - ox,
                                poy - oy,
                                parent.as_ref(),
                                &tiny_skia::PixmapPaint {
                                    opacity: 1.0,
                                    blend_mode: tiny_skia::BlendMode::Source,
                                    quality: tiny_skia::FilterQuality::Nearest,
                                },
                                tiny_skia::Transform::identity(),
                                None,
                            );
                            crate::primitives::gaussian_blur(
                                l.data_mut(),
                                bw,
                                bh,
                                *backdrop_blur,
                                &mut self.blur_scratch,
                            );
                        } else {
                            l.fill(tiny_skia::Color::TRANSPARENT);
                        }
                        self.layer_stack.push((l, *opacity, ox, oy, *clip_radius));
                    }
                }
                DrawCommand::PopLayer => {
                    if let Some((mut layer, opacity, ox, oy, clip_radius)) = self.layer_stack.pop()
                    {
                        if clip_radius > 0.0 {
                            let w = layer.width() as f32;
                            let h = layer.height() as f32;
                            let r = clip_radius;
                            let mut pb = tiny_skia::PathBuilder::new();
                            pb.move_to(r, 0.0);
                            pb.line_to(w - r, 0.0);
                            pb.quad_to(w, 0.0, w, r);
                            pb.line_to(w, h - r);
                            pb.quad_to(w, h, w - r, h);
                            pb.line_to(r, h);
                            pb.quad_to(0.0, h, 0.0, h - r);
                            pb.line_to(0.0, r);
                            pb.quad_to(0.0, 0.0, r, 0.0);
                            pb.close();
                            if let Some(path) = pb.finish() {
                                let mut paint = tiny_skia::Paint::default();
                                paint.set_color(tiny_skia::Color::WHITE);
                                paint.blend_mode = tiny_skia::BlendMode::DestinationIn;
                                layer.fill_path(
                                    &path,
                                    &paint,
                                    tiny_skia::FillRule::Winding,
                                    tiny_skia::Transform::identity(),
                                    None,
                                );
                            }
                        }
                        let (parent_ox, parent_oy) = self
                            .layer_stack
                            .last()
                            .map(|(_, _, pox, poy, _)| (*pox, *poy))
                            .unwrap_or((0, 0));
                        let target = if let Some((top, _, _, _, _)) = self.layer_stack.last_mut() {
                            top
                        } else {
                            self.pixmap.as_mut().unwrap()
                        };
                        target.draw_pixmap(
                            ox - parent_ox,
                            oy - parent_oy,
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

        self.present_pixmap()
    }
}
