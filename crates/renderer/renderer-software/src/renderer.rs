use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::mpsc;

use clru::{CLruCache, CLruCacheConfig};
use geometry_core::Rect;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use renderer_core::{
    Color, DrawCommand, RenderBackend, RendererError, expand_fill_layers, union_rects,
};
use renderer_text::{TextShaper, TextShaperConfig};
use rustc_hash::{FxBuildHasher, FxHasher};
use smallvec::SmallVec;
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

fn cull_bounds(vr: geometry_core::Rect, clip: Option<geometry_core::Rect>) -> bool {
    !renderer_core::culling::overlaps(vr.x, vr.y, vr.width, vr.height, clip)
}

fn fill_mask_region(data: &mut [u8], stride: usize, region: (u32, u32, u32, u32), value: u8) {
    let (x0, y0, x1, y1) = region;
    let row_len = (x1 - x0) as usize;
    for y in y0..y1 {
        let start = y as usize * stride + x0 as usize;
        data[start..start + row_len].fill(value);
    }
}

fn union_opt_rect(acc: Option<Rect>, r: Rect) -> Option<Rect> {
    Some(match acc {
        None => r,
        Some(a) => union_rects(a, r),
    })
}

fn compute_layer_bboxes(
    commands: &[DrawCommand],
    window_w: u32,
    window_h: u32,
    font_metrics: &renderer_core::FontMetrics,
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
                    // Propagate this layer's visual footprint to the parent layer's accumulator so
                    // the parent layer is sized to contain the composited result of all nested layers.
                    if !stack.is_empty() {
                        let footprint = Rect {
                            x: ox as f32,
                            y: oy as f32,
                            width: bw as f32,
                            height: bh as f32,
                        };
                        let last = stack.len() - 1;
                        stack[last].1 = union_opt_rect(stack[last].1, footprint);
                    }
                }
            }
            _ => {
                if let Some(vr) =
                    renderer_core::culling::command_visual_rect(cmd, cum_matrix, font_metrics)
                {
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
        let shift = dx.unsigned_abs() as usize;
        let copy_cols = (x1 - x0).saturating_sub(shift);
        if copy_cols > 0 {
            let byte_count = copy_cols * 4;
            if dx < 0 {
                // Content moved left: copy columns [x0+shift..x1] → [x0..x0+copy_cols] per row.
                for y in y0..y1 {
                    let row_base = y * width;
                    let src_off = (row_base + x0 + shift) * 4;
                    let dst_off = (row_base + x0) * 4;
                    data.copy_within(src_off..src_off + byte_count, dst_off);
                }
            } else {
                // Content moved right: copy columns [x0..x0+copy_cols] → [x0+shift..x1] per row.
                for y in y0..y1 {
                    let row_base = y * width;
                    let src_off = (row_base + x0) * 4;
                    let dst_off = (row_base + x0 + shift) * 4;
                    data.copy_within(src_off..src_off + byte_count, dst_off);
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

fn fill_rounded_mask(
    mask: &mut tiny_skia::Mask,
    rect: geometry_core::Rect,
    radius: renderer_core::BorderRadius,
) {
    let (x, y, w, h) = (rect.x, rect.y, rect.width, rect.height);
    let (tl, tr, br, bl) = (
        radius.top_left,
        radius.top_right,
        radius.bottom_right,
        radius.bottom_left,
    );
    let k = renderer_core::BEZIER_CIRCLE_K;
    let mut pb = tiny_skia::PathBuilder::new();
    pb.move_to(x + tl, y);
    pb.line_to(x + w - tr, y);
    pb.cubic_to(
        x + w - tr * (1.0 - k),
        y,
        x + w,
        y + tr * (1.0 - k),
        x + w,
        y + tr,
    );
    pb.line_to(x + w, y + h - br);
    pb.cubic_to(
        x + w,
        y + h - br * (1.0 - k),
        x + w - br * (1.0 - k),
        y + h,
        x + w - br,
        y + h,
    );
    pb.line_to(x + bl, y + h);
    pb.cubic_to(
        x + bl * (1.0 - k),
        y + h,
        x,
        y + h - bl * (1.0 - k),
        x,
        y + h - bl,
    );
    pb.line_to(x, y + tl);
    pb.cubic_to(x, y + tl * (1.0 - k), x + tl * (1.0 - k), y, x + tl, y);
    pb.close();
    if let Some(path) = pb.finish() {
        mask.fill_path(
            &path,
            tiny_skia::FillRule::Winding,
            true,
            tiny_skia::Transform::identity(),
        );
    }
}

// Hashes the draw-command slice together with the viewport dimensions, used to key the layer-bbox cache which depends on both the commands and the surface size.
fn hash_commands_with_dims(commands: &[DrawCommand], width: u32, height: u32) -> u64 {
    let mut h = FxHasher::default();
    width.hash(&mut h);
    height.hash(&mut h);
    renderer_core::hash_draw_commands_into(commands, &mut h);
    h.finish()
}

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
    clip_mask_buf: Option<tiny_skia::Mask>,
    // Last region written as 0xFF into clip_mask_buf. Tracked across frames so the next PushClip can zero stale bits left by the previous frame without re-zeroing the whole mask.
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
    // Cache for compute_layer_bboxes: avoids re-traversing commands when input and dimensions are unchanged.
    layer_bboxes_cache: Option<(u64, Vec<Option<(i32, i32, u32, u32)>>)>,
}

impl<D, W> SoftwareRenderer<D, W>
where
    D: HasDisplayHandle,
    W: HasWindowHandle,
{
    pub fn new(
        display: D,
        window: W,
        budget: crate::SoftwareRendererConfig,
    ) -> Result<Self, RendererError> {
        let context = Context::new(display).map_err(|e| {
            RendererError::Backend(format!("softbuffer context creation failed: {}", e))
        })?;
        let surface =
            Surface::new(&context, window).map_err(|e| RendererError::Surface(e.to_string()))?;
        let mut text_shaper = TextShaper::with_config(TextShaperConfig {
            pixel_cache_budget_bytes: budget.text_pixel_cache_bytes,
            alpha_cache_budget_bytes: budget.text_alpha_cache_bytes,
            shaping_cache_budget_bytes: budget.text_shaping_cache_bytes,
            font: budget.font,
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
            pending_shadows: HashMap::new(),
            pending_text_shadows: HashMap::new(),
            pending_path_shadows: HashMap::new(),
            layer_stack: Vec::new(),
            prev_commands: Vec::with_capacity(256),
            prev_commands_hash: 0,
            prev_clear_color: None,
            expanded_commands_cache: None,
            layer_bboxes_cache: None,
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

    fn present_pixmap(&mut self) -> Result<(), RendererError> {
        let Some(pixmap) = &self.pixmap else {
            return Ok(());
        };
        if self.width == 0 || self.height == 0 {
            return Ok(());
        }
        if let Ok(mut buffer) = self.surface.buffer_mut() {
            // Pixel format: tiny_skia RGBA bytes → softbuffer LE u32 0x00RRGGBB.
            // u32 bit trick: output = (input>>16 & 0xFF) | (input & 0xFF00) | ((input & 0xFF)<<16)
            #[cfg(target_endian = "little")]
            {
                use wide::u32x8;
                let src = pixmap.data();
                let pixels = buffer.len();
                let mask_lo = u32x8::splat(0xFF);
                let mask_g = u32x8::splat(0x0000_FF00);
                let shift16 = u32x8::splat(16);
                let n_simd = pixels / 8;
                for i in 0..n_simd {
                    let b = i * 8;
                    let v = u32x8::from(std::array::from_fn::<u32, 8, _>(|j| {
                        let p = (b + j) * 4;
                        u32::from_le_bytes(src[p..p + 4].try_into().unwrap())
                    }));
                    let out =
                        ((v >> shift16) & mask_lo) | (v & mask_g) | ((v & mask_lo) << shift16);
                    let out_arr: [u32; 8] = out.into();
                    buffer[b..b + 8].copy_from_slice(&out_arr);
                }
                for i in (n_simd * 8)..pixels {
                    let p = i * 4;
                    let s = u32::from_le_bytes(src[p..p + 4].try_into().unwrap());
                    buffer[i] = ((s >> 16) & 0xFF) | (s & 0xFF00) | ((s & 0xFF) << 16);
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
    fn begin_frame(
        &mut self,
        width: u32,
        height: u32,
        _scale_factor: f32,
        _generation: u64,
    ) -> Result<(), RendererError> {
        // `scale_factor` and `generation` are ignored because draw commands arrive pre-scaled by the caller; software backend does not need to track them.

        if width != self.width || height != self.height {
            self.width = width;
            self.height = height;
            self.pixmap = Pixmap::new(width, height);
            self.clip_mask_buf = tiny_skia::Mask::new(width, height);
            self.clip_mask_dirty = None;
            self.pixmap_pool.clear();
            self.prev_commands.clear();
            self.prev_commands_hash = 0;
            self.prev_clear_color = None;
            self.expanded_commands_cache = None;
            self.layer_bboxes_cache = None;
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
        // Poll background shadow workers and move finished pixmaps into their caches. Returns true if any completed this frame, in which case we must re-render even if the command list is unchanged so the newly-available shadow gets drawn.
        let shadow_arrived = self.poll_pending_shadows();

        // Optimization 1: skip the entire render when nothing changed; just re-present the existing pixmap. A shadow that just finished computing forces a redraw so it can appear.
        if !shadow_arrived
            && commands == self.prev_commands.as_slice()
            && clear_color == self.prev_clear_color
        {
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

        // Optimization 3: compute the on-screen regions that changed so we can clear and re-render only those. Disjoint changes (e.g. a header and a scrollbar) are kept as separate rects instead of a viewport-spanning union, so the untouched center can be skipped.
        let dirty_rect: Option<SmallVec<[Rect; 8]>> = if let Some(ref sb) = maybe_scroll {
            // Scroll blit case: only re-render the newly exposed band and any changed overlays.
            let mut v: SmallVec<[Rect; 8]> = SmallVec::new();
            v.push(sb.exposed_band);
            if let Some(ed) = sb.extra_dirty {
                v.push(ed);
            }
            Some(v)
        } else if self.prev_commands.is_empty() {
            None // first frame → full clear
        } else {
            renderer_core::dirty::compute_dirty_rect(commands, &self.prev_commands, |cmd, m| {
                renderer_core::culling::command_visual_rect(cmd, m, &self.font_metrics)
            })
        };

        let clear_color_changed = clear_color != self.prev_clear_color;
        let current_hash = renderer_core::hash_draw_commands(commands);
        if current_hash != self.prev_commands_hash {
            self.prev_commands.clear();
            self.prev_commands.extend(commands.iter().cloned());
            self.prev_commands_hash = current_hash;
        }
        self.prev_clear_color = clear_color;

        // Clear either the dirty regions only or the full pixmap when a structural change forces a full re-render; IMPORTANT: compute both the tiny-skia clear rect and the geometry rect used for command-skipping from the same clamped bounds because the naive (dr.x-1).max(0) / dr.width+2 formula shifts the rect right/down when dr has negative coordinates (off-screen content), so fill_rect would clear a larger on-screen area than `dr` describes — causing commands outside `dr` to have their pixels cleared and then be skipped, which makes them disappear.
        let skip_rect: Option<SmallVec<[Rect; 8]>> = match dirty_rect {
            Some(drs) if !drs.is_empty() => {
                // Precompute each command's window-space visual rect once so expanding every dirty region is O(rects + commands) rather than O(rects * commands).
                let mut visual_rects: Vec<Rect> = Vec::with_capacity(commands.len());
                renderer_core::for_each_with_matrix(commands, |cmd, matrix| {
                    if let Some(vr) =
                        renderer_core::culling::command_visual_rect(cmd, matrix, &self.font_metrics)
                    {
                        visual_rects.push(vr);
                    }
                });

                let mut out: SmallVec<[Rect; 8]> = SmallVec::new();
                for dr in drs.iter() {
                    if dr.width <= 0.0 || dr.height <= 0.0 {
                        continue;
                    }
                    let x0 = (dr.x - 1.0).max(0.0);
                    let y0 = (dr.y - 1.0).max(0.0);
                    let x1 = (dr.x + dr.width + 1.0).min(self.width as f32);
                    let y1 = (dr.y + dr.height + 1.0).min(self.height as f32);
                    if x1 <= x0 || y1 <= y0 {
                        // This region is entirely off-screen; skip it.
                        continue;
                    }
                    // Expand the region to fully contain every command it partially intersects: a partially-overlapping command is still fully redrawn, overwriting pixels of earlier commands that fall outside the region and won't be redrawn themselves.
                    let mut sr = Rect {
                        x: x0,
                        y: y0,
                        width: x1 - x0,
                        height: y1 - y0,
                    };
                    // A single pass is insufficient when expansion brings new commands into range; iterate until the region stops growing (bounded by command count in the worst case, but converges in 1-2 passes in practice).
                    loop {
                        let before = sr;
                        for vr in &visual_rects {
                            if vr.overlaps(sr) {
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
                        if sr == before {
                            break;
                        }
                    }
                    // Re-clamp to viewport after expansion.
                    let fx0 = sr.x.max(0.0);
                    let fy0 = sr.y.max(0.0);
                    let fx1 = (sr.x + sr.width).min(self.width as f32);
                    let fy1 = (sr.y + sr.height).min(self.height as f32);
                    if fx1 > fx0 && fy1 > fy0 {
                        out.push(Rect {
                            x: fx0,
                            y: fy0,
                            width: fx1 - fx0,
                            height: fy1 - fy0,
                        });
                    }
                }
                if out.is_empty() {
                    // Every dirty region was off-screen — nothing visible changed.
                    return self.present_pixmap();
                }
                Some(out)
            }
            _ => None,
        };

        // If the clear color changed, the dirty-rect only covers command-changed regions, leaving
        // background areas untouched with stale pixels from the previous frame. Force a full clear.
        let skip_rect = if clear_color_changed { None } else { skip_rect };

        if let (Some(color), Some(pixmap)) = (clear_color, &mut self.pixmap) {
            if let Some(ref rects) = skip_rect {
                for sr in rects.iter() {
                    let skia_rect = tiny_skia::Rect::from_xywh(sr.x, sr.y, sr.width, sr.height);
                    if let Some(r) = skia_rect {
                        let mut paint = tiny_skia::Paint::default();
                        paint.set_color(crate::primitives::to_skia_color(color));
                        paint.blend_mode = tiny_skia::BlendMode::Source;
                        pixmap.fill_rect(r, &paint, tiny_skia::Transform::identity(), None);
                    } else {
                        pixmap.fill(crate::primitives::to_skia_color(color));
                        break;
                    }
                }
            } else {
                pixmap.fill(crate::primitives::to_skia_color(color));
            }
        }

        self.draw_state.reset();
        self.layer_stack.clear();

        let input_hash = current_hash;
        let expanded_commands = match &self.expanded_commands_cache {
            Some((cached_hash, _)) if *cached_hash == input_hash => None, // signals "use cache"
            _ => {
                let result = expand_fill_layers(commands);
                // Store the expanded result (or a clone of the original if no expansion needed).
                let stored = result.clone().unwrap_or_else(|| commands.to_vec());
                self.expanded_commands_cache = Some((input_hash, stored));
                result
            }
        };
        // When expanded_commands is None and cache hit occurred, we still need to know if expansion happened.
        // The cache stores the final resolved slice (expanded or original), so we use it directly.
        let commands: &[DrawCommand] = match &expanded_commands {
            Some(v) => v.as_slice(),
            None => {
                // Either no semi-transparent rects (expand_fill_layers returned None) OR cache hit.
                // In both cases the cache holds the correct resolved slice.
                if let Some((cached_hash, cached)) = &self.expanded_commands_cache {
                    if *cached_hash == input_hash {
                        cached.as_slice()
                    } else {
                        commands
                    }
                } else {
                    commands
                }
            }
        };

        // Task 2.12: skip compute_layer_bboxes when commands and dimensions haven't changed.
        let bbox_hash = hash_commands_with_dims(commands, self.width, self.height);
        let layer_bboxes = match &self.layer_bboxes_cache {
            Some((cached_hash, cached)) if *cached_hash == bbox_hash => cached.clone(),
            _ => {
                let result =
                    compute_layer_bboxes(commands, self.width, self.height, &self.font_metrics);
                self.layer_bboxes_cache = Some((bbox_hash, result.clone()));
                result
            }
        };

        // Nesting depth of PushLayer commands skipped because their bbox doesn't overlap skip_rect; their pixels are already correct from apply_scroll_blit.
        let mut skip_layer_depth: usize = 0;

        for (cmd_idx, cmd) in commands.iter().enumerate() {
            if skip_layer_depth > 0 {
                match cmd {
                    DrawCommand::PushLayer { .. } => skip_layer_depth += 1,
                    DrawCommand::PopLayer => skip_layer_depth -= 1,
                    _ => {}
                }
                continue;
            }

            if self.pixmap.is_none() {
                break;
            }

            let inside_layer = !self.layer_stack.is_empty();
            let (layer_ox, layer_oy) = self
                .layer_stack
                .last()
                .map(|(_, _, ox, oy)| (*ox, *oy))
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

            // Optimization 3: skip draw commands whose visual bounds don't overlap the dirty region.
            // Only applies at the top level (not inside layers): a layer is a fresh isolated pixmap
            // rendered from scratch every frame, so all its commands must run regardless of which
            // window-space region is dirty.
            if let Some(ref dirty_rects) = skip_rect {
                if !inside_layer {
                    if let Some(vr) = renderer_core::culling::command_visual_rect(
                        cmd,
                        self.draw_state.cum_matrix,
                        &self.font_metrics,
                    ) {
                        if dirty_rects.iter().all(|dr| !vr.overlaps(*dr)) {
                            continue;
                        }
                    }
                }
            }

            match cmd {
                DrawCommand::Rect { rect, style } => {
                    let rect = *rect;
                    let style = **style;
                    if rect.width <= 0.0
                        || rect.height <= 0.0
                        || (style.fill.is_none() && style.stroke.is_none())
                    {
                        continue;
                    }
                    if let Some(vr) = renderer_core::culling::command_visual_rect(
                        cmd,
                        self.draw_state.cum_matrix,
                        &self.font_metrics,
                    ) {
                        if cull_bounds(vr, self.draw_state.current_clip()) {
                            continue;
                        }
                    }
                    let pixmap = if let Some((layer, _, _, _)) = self.layer_stack.last_mut() {
                        layer
                    } else {
                        self.pixmap.as_mut().unwrap()
                    };
                    let clip = if self.draw_state.current_clip().is_some() && !inside_layer {
                        self.clip_mask_buf.as_ref()
                    } else {
                        None
                    };
                    crate::primitives::rect::draw_rect(
                        pixmap,
                        rect,
                        &style,
                        transform,
                        clip,
                        &mut self.shadow_cache,
                        &mut self.pending_shadows,
                        &mut self.blur_scratch,
                    );
                }
                DrawCommand::Text { text, rect, style } => {
                    let rect = *rect;
                    let style = **style;
                    if let Some(vr) = renderer_core::culling::command_visual_rect(
                        cmd,
                        self.draw_state.cum_matrix,
                        &self.font_metrics,
                    ) {
                        if cull_bounds(vr, self.draw_state.current_clip()) {
                            continue;
                        }
                    }
                    let pixmap = if let Some((top, _, _, _)) = self.layer_stack.last_mut() {
                        top
                    } else {
                        self.pixmap.as_mut().unwrap()
                    };
                    let clip = if self.draw_state.current_clip().is_some() && !inside_layer {
                        self.clip_mask_buf.as_ref()
                    } else {
                        None
                    };
                    crate::primitives::text::draw_text(
                        pixmap,
                        &mut self.text_shaper,
                        text,
                        rect,
                        &style,
                        transform,
                        clip,
                        if inside_layer {
                            None
                        } else {
                            self.draw_state.current_clip()
                        },
                        &mut self.blur_scratch,
                        &mut self.text_pixmap_cache,
                        &mut self.text_shadow_cache,
                        &mut self.pending_text_shadows,
                    );
                }
                DrawCommand::Image { data, rect, filter } => {
                    if let Some(vr) = renderer_core::culling::command_visual_rect(
                        cmd,
                        self.draw_state.cum_matrix,
                        &self.font_metrics,
                    ) {
                        if cull_bounds(vr, self.draw_state.current_clip()) {
                            continue;
                        }
                    }
                    let pixmap = if let Some((top, _, _, _)) = self.layer_stack.last_mut() {
                        top
                    } else {
                        self.pixmap.as_mut().unwrap()
                    };
                    let clip = if self.draw_state.current_clip().is_some() && !inside_layer {
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
                    if let Some(vr) = renderer_core::culling::command_visual_rect(
                        cmd,
                        self.draw_state.cum_matrix,
                        &self.font_metrics,
                    ) {
                        if cull_bounds(vr, self.draw_state.current_clip()) {
                            continue;
                        }
                    }
                    let pixmap = if let Some((top, _, _, _)) = self.layer_stack.last_mut() {
                        top
                    } else {
                        self.pixmap.as_mut().unwrap()
                    };
                    let clip = if self.draw_state.current_clip().is_some() && !inside_layer {
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
                        if inside_layer {
                            None
                        } else {
                            self.draw_state.current_clip()
                        },
                    );
                }
                DrawCommand::Path { data, style } => {
                    let style = **style;
                    if let Some(vr) = renderer_core::culling::command_visual_rect(
                        cmd,
                        self.draw_state.cum_matrix,
                        &self.font_metrics,
                    ) {
                        if cull_bounds(vr, self.draw_state.current_clip()) {
                            continue;
                        }
                    }
                    let pixmap = if let Some((top, _, _, _)) = self.layer_stack.last_mut() {
                        top
                    } else {
                        self.pixmap.as_mut().unwrap()
                    };
                    let clip = if self.draw_state.current_clip().is_some() && !inside_layer {
                        self.clip_mask_buf.as_ref()
                    } else {
                        None
                    };
                    crate::primitives::path::draw_path(
                        pixmap,
                        data,
                        &style,
                        transform,
                        clip,
                        if inside_layer {
                            None
                        } else {
                            self.draw_state.current_clip()
                        },
                        &mut self.blur_scratch,
                        &mut self.path_shadow_cache,
                        &mut self.pending_path_shadows,
                    );
                }
                DrawCommand::PushClip { rect, radius } => {
                    let prev_dirty = self.clip_mask_dirty;
                    let effective = self.draw_state.push_clip(*rect);
                    if let Some(ref mut m) = self.clip_mask_buf {
                        if radius.is_zero() {
                            repaint_mask(m, effective, prev_dirty, self.width, self.height);
                        } else {
                            if let Some(prev) = prev_dirty {
                                if prev != effective {
                                    if let Some(region) =
                                        clamp_to_pixels(prev, self.width, self.height)
                                    {
                                        fill_mask_region(
                                            m.data_mut(),
                                            self.width as usize,
                                            region,
                                            0,
                                        );
                                    }
                                }
                            }
                            fill_rounded_mask(m, effective, *radius);
                        }
                    }
                    self.clip_mask_dirty = Some(effective);
                }
                DrawCommand::PopClip => {
                    let prev_dirty = self.clip_mask_dirty;
                    let effective = self.draw_state.pop_clip();
                    match effective {
                        Some(r) => {
                            if let Some(ref mut m) = self.clip_mask_buf {
                                repaint_mask(m, r, prev_dirty, self.width, self.height);
                            }
                            self.clip_mask_dirty = Some(r);
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
                } => {
                    // During scroll_blit, skip layers outside the dirty region: their pixels are already correct from apply_scroll_blit and re-compositing would double-apply the layer's opacity.
                    if let Some(ref dirty_rects) = skip_rect {
                        if !inside_layer {
                            if let Some((ox, oy, bw, bh)) = layer_bboxes[cmd_idx] {
                                let layer_rect = Rect {
                                    x: ox as f32,
                                    y: oy as f32,
                                    width: bw as f32,
                                    height: bh as f32,
                                };
                                if dirty_rects.iter().all(|dr| !layer_rect.overlaps(*dr)) {
                                    skip_layer_depth = 1;
                                    continue;
                                }
                            }
                        }
                    }
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
                                .map(|(_, _, pox, poy)| (*pox, *poy))
                                .unwrap_or((0, 0));
                            let parent = if let Some((top, _, _, _)) = self.layer_stack.last() {
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
                        self.layer_stack.push((l, *opacity, ox, oy));
                    }
                }
                DrawCommand::PopLayer => {
                    if let Some((layer, opacity, ox, oy)) = self.layer_stack.pop() {
                        let (parent_ox, parent_oy) = self
                            .layer_stack
                            .last()
                            .map(|(_, _, pox, poy)| (*pox, *poy))
                            .unwrap_or((0, 0));
                        let target = if let Some((top, _, _, _)) = self.layer_stack.last_mut() {
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
                #[cfg(target_os = "android")]
                DrawCommand::AndroidHardwareBufferImage { .. } => {
                    debug_assert!(
                        false,
                        "AndroidHardwareBufferImage is not supported in the software renderer"
                    );
                }
            }
        }

        self.present_pixmap()
    }
}
