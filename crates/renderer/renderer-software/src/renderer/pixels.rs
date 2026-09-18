use std::hash::{Hash, Hasher};
use std::ops::Range;

use geometry_core::Rect;
use renderer_core::culling::{PaintBounds, command_visual_rect};
use renderer_core::{DrawCommand, DrawState, FontMetrics, transform_clip_rect};
use rustc_hash::FxHasher;
use tiny_skia::Pixmap;

#[cfg(target_endian = "big")]
compile_error!(
    "the present-buffer pixel conversion reads tiny_skia's RGBA bytes as little-endian words; big-endian targets are not supported"
);

// `(x0, y0, x1, y1)`, the end exclusive.
pub(super) type PixelBounds = (u32, u32, u32, u32);

pub(super) type LayerBox = (i32, i32, u32, u32);

pub(super) fn clamp_to_pixels(rect: Rect, width: u32, height: u32) -> Option<PixelBounds> {
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

pub(super) fn fill_mask_region(data: &mut [u8], stride: usize, region: PixelBounds, value: u8) {
    let (x0, y0, x1, y1) = region;
    let row_len = (x1 - x0) as usize;
    for y in y0..y1 {
        let start = y as usize * stride + x0 as usize;
        data[start..start + row_len].fill(value);
    }
}

// The coverage product tiny_skia's own `Mask::intersect_path` takes, over one region rather than a whole mask.
pub(super) fn multiply_mask_region(data: &mut [u8], by: &[u8], stride: usize, region: PixelBounds) {
    let (x0, y0, x1, y1) = region;
    for y in y0 as usize..y1 as usize {
        let row = y * stride;
        for at in row + x0 as usize..row + x1 as usize {
            let product = u32::from(data[at]) * u32::from(by[at]) + 128;
            data[at] = ((product + (product >> 8)) >> 8) as u8;
        }
    }
}

pub(super) fn fill_region(
    pixmap: &mut Pixmap,
    region: Rect,
    color: tiny_skia::PremultipliedColorU8,
) {
    let Some((x0, y0, x1, y1)) = clamp_to_pixels(region, pixmap.width(), pixmap.height()) else {
        return;
    };
    let stride = pixmap.width() as usize;
    let pixels = pixmap.pixels_mut();
    for y in y0 as usize..y1 as usize {
        pixels[y * stride + x0 as usize..y * stride + x1 as usize].fill(color);
    }
}

// `None` for a layer that paints nothing.
pub(super) fn compute_layer_bounds(
    commands: &[DrawCommand],
    window_w: u32,
    window_h: u32,
    font_metrics: &FontMetrics,
) -> Vec<Option<LayerBox>> {
    let mut boxes = vec![None; commands.len()];
    let mut state = DrawState::new();
    let mut layers = PaintBounds::new();
    let surface = Rect::new(0.0, 0.0, window_w as f32, window_h as f32);
    for (index, cmd) in commands.iter().enumerate() {
        match cmd {
            DrawCommand::PushMatrix { matrix } => state.push_matrix(*matrix),
            DrawCommand::PopMatrix => state.pop_matrix(),
            DrawCommand::PushClip { rect, .. } => {
                state.push_clip(transform_clip_rect(state.cumulative_matrix, *rect));
            }
            DrawCommand::PopClip => {
                state.pop_clip();
            }
            DrawCommand::PushLayer { backdrop_blur, .. } => {
                layers.open((index, *backdrop_blur > 0.0));
            }
            DrawCommand::PopLayer => {
                let Some(((opened, blurs_backdrop), painted)) = layers.close() else {
                    continue;
                };
                let footprint = match painted {
                    Some(bounds) => clamp_to_pixels(bounds, window_w, window_h),
                    None if blurs_backdrop => clamp_to_pixels(surface, window_w, window_h),
                    None => None,
                };
                if let Some((x0, y0, x1, y1)) = footprint {
                    boxes[opened] = Some((x0 as i32, y0 as i32, x1 - x0, y1 - y0));
                    layers.include(Rect::new(
                        x0 as f32,
                        y0 as f32,
                        (x1 - x0) as f32,
                        (y1 - y0) as f32,
                    ));
                }
            }
            _ => {
                let clip = state.current_clip();
                if let Some(painted) =
                    command_visual_rect(cmd, state.cumulative_matrix, font_metrics)
                        .and_then(|rect| clip.map_or(Some(rect), |clip| clip.intersect(rect)))
                {
                    layers.include(painted);
                }
            }
        }
    }
    boxes
}

// The two are mutually exclusive. The newly exposed strip is left stale for the caller to re-render.
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
            // Content moved up: write to a lower row, read from a higher one, so top-to-bottom is safe.
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
            // Content moved down: write to a higher row, read from a lower one, so bottom-to-top is safe.
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
                // Content moved left: copy columns `[x0+shift..x1]` to `[x0..x0+copy_cols]` per row.
                for y in y0..y1 {
                    let row_base = y * width;
                    let src_off = (row_base + x0 + shift) * 4;
                    let dst_off = (row_base + x0) * 4;
                    data.copy_within(src_off..src_off + byte_count, dst_off);
                }
            } else {
                // Content moved right: copy columns `[x0..x0+copy_cols]` to `[x0+shift..x1]` per row.
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

// Keys the layer-bbox cache, which depends on both the commands and the surface size.
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

#[derive(Clone, Copy)]
pub(super) enum PixelFormat {
    Xrgb8888,
    #[cfg(target_os = "linux")]
    Argb8888,
}

impl PixelFormat {
    // tiny_skia's RGBA bytes read as a u32 are `0xAABBGGRR`: red and blue trade places, green stays, and alpha stays only in a format that carries it.
    fn kept_bits(self) -> u32 {
        match self {
            PixelFormat::Xrgb8888 => 0x0000_FF00,
            #[cfg(target_os = "linux")]
            PixelFormat::Argb8888 => 0xFF00_FF00,
        }
    }
}

#[inline(always)]
fn swizzle_word(s: u32, kept: u32) -> u32 {
    ((s >> 16) & 0xFF) | (s & kept) | ((s & 0xFF) << 16)
}

// Both slices hold the same pixel count.
fn swizzle_words(src: &[u32], dst: &mut [u32], kept: u32) {
    use wide::u32x8;
    let mask_lo = u32x8::splat(0xFF);
    let mask_kept = u32x8::splat(kept);
    let shift16 = u32x8::splat(16);
    let (src_chunks, src_rest) = src.as_chunks::<8>();
    let (dst_chunks, dst_rest) = dst.as_chunks_mut::<8>();
    for (s, d) in src_chunks.iter().zip(dst_chunks) {
        let v = u32x8::from(*s);
        let out = ((v >> shift16) & mask_lo) | (v & mask_kept) | ((v & mask_lo) << shift16);
        d.copy_from_slice(&<[u32; 8]>::from(out));
    }
    for (s, d) in src_rest.iter().zip(dst_rest) {
        *d = swizzle_word(*s, kept);
    }
}

// `dst.len()` pixels are written; `src` must hold four times that in bytes. Reads the RGBA bytes as packed u32 words to avoid a per-pixel byte gather, falling back to a scalar gather when unaligned.
pub(super) fn convert_rgba(src: &[u8], dst: &mut [u32], format: PixelFormat) {
    let kept = format.kept_bits();
    let pixels = dst.len();
    let bytes = &src[..pixels * 4];
    // SAFETY: any byte pattern is a valid u32, and only the aligned middle is read.
    let (pre, words, _post) = unsafe { bytes.align_to::<u32>() };
    if pre.is_empty() && words.len() >= pixels {
        swizzle_words(&words[..pixels], dst, kept);
        return;
    }
    for (px, d) in bytes.as_chunks::<4>().0.iter().zip(dst.iter_mut()) {
        *d = swizzle_word(u32::from_le_bytes(*px), kept);
    }
}

// A full-width rect is one contiguous span, the common case for a horizontal scroll band; a narrower rect is a span per row.
fn region_spans(width: usize, height: usize, rect: Rect, mut f: impl FnMut(Range<usize>)) {
    let Some((x0, y0, x1, y1)) = clamp_to_pixels(rect, width as u32, height as u32) else {
        return;
    };
    let (x0, y0, x1, y1) = (x0 as usize, y0 as usize, x1 as usize, y1 as usize);
    if x0 == 0 && x1 == width {
        f(y0 * width..y1 * width);
        return;
    }
    for y in y0..y1 {
        let row = y * width;
        f(row + x0..row + x1);
    }
}

pub(super) fn convert_rgba_region(
    src: &[u8],
    dst: &mut [u32],
    width: usize,
    height: usize,
    rect: Rect,
    format: PixelFormat,
) {
    region_spans(width, height, rect, |span| {
        convert_rgba(&src[span.start * 4..span.end * 4], &mut dst[span], format)
    });
}

#[cfg(target_os = "linux")]
pub(super) fn copy_region(src: &[u32], dst: &mut [u32], width: usize, height: usize, rect: Rect) {
    region_spans(width, height, rect, |span| {
        dst[span.clone()].copy_from_slice(&src[span])
    });
}
