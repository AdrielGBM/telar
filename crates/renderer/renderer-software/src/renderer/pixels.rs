use std::hash::{Hash, Hasher};

use geometry_core::Rect;
use renderer_core::DrawCommand;
use rustc_hash::FxHasher;
use tiny_skia::Pixmap;

pub(super) fn clamp_to_pixels(rect: Rect, width: u32, height: u32) -> Option<(u32, u32, u32, u32)> {
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

pub(super) fn cull_bounds(vr: geometry_core::Rect, clip: Option<geometry_core::Rect>) -> bool {
    !renderer_core::culling::overlaps(vr.x, vr.y, vr.width, vr.height, clip)
}

pub(super) fn fill_mask_region(
    data: &mut [u8],
    stride: usize,
    region: (u32, u32, u32, u32),
    value: u8,
) {
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
        Some(a) => a.union(r),
    })
}

pub(super) fn compute_layer_bounds(
    commands: &[DrawCommand],
    window_w: u32,
    window_h: u32,
    font_metrics: &renderer_core::FontMetrics,
) -> Vec<Option<(i32, i32, u32, u32)>> {
    let mut result = vec![None; commands.len()];
    let mut stack: Vec<(usize, Option<Rect>)> = Vec::new();

    // for_each_with_matrix owns the PushMatrix/PopMatrix cumulative-matrix walk; the callback keeps only the layer-stack bounds accumulation. `idx` mirrors the command position since the callback fires once per command in order.
    let mut idx = 0usize;
    renderer_core::for_each_with_matrix(commands, |cmd, matrix| {
        match cmd {
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
                    // Propagate this layer's visual footprint to the parent layer's accumulator so the parent layer is sized to contain the composited result of all nested layers.
                    if let Some(parent) = stack.last_mut() {
                        let footprint = Rect {
                            x: ox as f32,
                            y: oy as f32,
                            width: bw as f32,
                            height: bh as f32,
                        };
                        parent.1 = union_opt_rect(parent.1, footprint);
                    }
                }
            }
            _ => {
                // command_visual_rect returns None for PushMatrix/PopMatrix/PushClip/PopClip, so those pass through without touching the accumulator.
                if let Some(vr) =
                    renderer_core::culling::command_visual_rect(cmd, matrix, font_metrics)
                {
                    if let Some(last) = stack.last_mut() {
                        last.1 = union_opt_rect(last.1, vr);
                    }
                }
            }
        }
        idx += 1;
    });

    result
}

// Shifts rows (Y scroll) or columns (X scroll) inside `clip` in place; the two are mutually exclusive. The newly exposed strip is left stale and must be re-rendered by the caller.
pub(super) fn apply_scroll_blit(pixmap: &mut Pixmap, clip: Rect, delta_tx: f32, delta_ty: f32) {
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
pub(super) fn repaint_mask(
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

pub(super) fn fill_rounded_mask(
    mask: &mut tiny_skia::Mask,
    rect: geometry_core::Rect,
    radius: renderer_core::BorderRadius,
) {
    if let Some(path) = crate::primitives::rect::build_rect_path(rect, radius) {
        mask.fill_path(
            &path,
            tiny_skia::FillRule::Winding,
            true,
            tiny_skia::Transform::identity(),
        );
    }
}

// Hashes the draw-command slice together with the viewport dimensions, used to key the layer-bbox cache which depends on both the commands and the surface size.
pub(super) fn hash_commands_with_dimensions(
    commands: &[DrawCommand],
    width: u32,
    height: u32,
) -> u64 {
    let mut h = FxHasher::default();
    width.hash(&mut h);
    height.hash(&mut h);
    renderer_core::hash_draw_commands_into(commands, &mut h);
    h.finish()
}

// Per-pixel swizzle: tiny_skia RGBA byte order (read as LE u32 = 0xAABBGGRR) → softbuffer's 0x00RRGGBB.
#[cfg(target_endian = "little")]
#[inline(always)]
fn xrgb_from_rgba_word(s: u32) -> u32 {
    ((s >> 16) & 0xFF) | (s & 0xFF00) | ((s & 0xFF) << 16)
}

// SIMD swizzle of a packed-RGBA u32 slice into XRGB u32s. Both slices hold the same pixel count.
#[cfg(target_endian = "little")]
fn swizzle_words(src: &[u32], dst: &mut [u32]) {
    use wide::u32x8;
    let mask_lo = u32x8::splat(0xFF);
    let mask_g = u32x8::splat(0x0000_FF00);
    let shift16 = u32x8::splat(16);
    let n_simd = dst.len() / 8;
    for i in 0..n_simd {
        let b = i * 8;
        let v = u32x8::from(<[u32; 8]>::try_from(&src[b..b + 8]).unwrap());
        let out = ((v >> shift16) & mask_lo) | (v & mask_g) | ((v & mask_lo) << shift16);
        let out_arr: [u32; 8] = out.into();
        dst[b..b + 8].copy_from_slice(&out_arr);
    }
    for i in (n_simd * 8)..dst.len() {
        dst[i] = xrgb_from_rgba_word(src[i]);
    }
}

// Converts a chunk of tiny_skia RGBA bytes into softbuffer's little-endian 0x00RRGGBB u32s. `dst.len()` pixels are written; `src` must hold `dst.len() * 4` bytes. Reads the RGBA bytes as packed u32 words (the pixmap allocation is 4-aligned in practice) to avoid a per-pixel byte gather; falls back to a scalar gather if the slice happens to be unaligned.
#[cfg(target_endian = "little")]
pub(super) fn convert_rgba_to_xrgb(src: &[u8], dst: &mut [u32]) {
    let pixels = dst.len();
    let bytes = &src[..pixels * 4];
    // SAFETY: any byte pattern is a valid u32; we only read the aligned middle.
    let (pre, words, _post) = unsafe { bytes.align_to::<u32>() };
    if pre.is_empty() && words.len() >= pixels {
        swizzle_words(words, dst);
        return;
    }
    // Unaligned fallback (rare): per-pixel byte gather.
    for i in 0..pixels {
        let p = i * 4;
        let s = u32::from_le_bytes(src[p..p + 4].try_into().unwrap());
        dst[i] = xrgb_from_rgba_word(s);
    }
}

// Swizzles only `rect` from the RGBA pixmap into the XRGB output buffer, reusing the SIMD converter. A full-width rect is swizzled as one contiguous block (the common case for a horizontal scroll band); narrower rects go row by row.
#[cfg(target_endian = "little")]
pub(super) fn convert_rgba_to_xrgb_region(
    src: &[u8],
    dst: &mut [u32],
    width: usize,
    height: usize,
    rect: Rect,
) {
    let Some((x0, y0, x1, y1)) = clamp_to_pixels(rect, width as u32, height as u32) else {
        return;
    };
    let (x0, y0, x1, y1) = (x0 as usize, y0 as usize, x1 as usize, y1 as usize);
    if x0 == 0 && x1 == width {
        // Rows are contiguous in memory — one SIMD pass over the whole span.
        let a = y0 * width;
        let b = y1 * width;
        convert_rgba_to_xrgb(&src[a * 4..b * 4], &mut dst[a..b]);
        return;
    }
    for y in y0..y1 {
        let row = y * width;
        convert_rgba_to_xrgb(
            &src[(row + x0) * 4..(row + x1) * 4],
            &mut dst[row + x0..row + x1],
        );
    }
}
