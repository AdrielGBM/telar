//! Blurring a shape into a cached pixmap, inline for small shadows and on a worker thread for large ones.

/// Quantizes blur radius to half-pixel steps so near-identical blurs share one shadow-cache entry.
pub(crate) fn quantize_blur(blur_radius: f32) -> f32 {
    (blur_radius * 2.0).round() / 2.0
}

/// Returns None if pixmap allocation fails.
pub(crate) fn render_shadow_pixmap(
    width: u32,
    height: u32,
    blur_radius: f32,
    blur_scratch: &mut Vec<u8>,
    draw_fn: impl FnOnce(&mut tiny_skia::Pixmap),
) -> Option<tiny_skia::Pixmap> {
    let mut pixmap = tiny_skia::Pixmap::new(width, height)?;
    draw_fn(&mut pixmap);
    let sigma = renderer_core::blur_sigma(blur_radius);
    gaussian_blur(pixmap.data_mut(), width, height, sigma, blur_scratch);
    Some(pixmap)
}

/// Shadow pixmaps larger than this many pixels are computed on a background thread; the result lands in the cache 1-2 frames later (the shadow is simply absent until then). Smaller shadows are computed synchronously since the spawn/blur overhead would dominate.
pub(crate) const ASYNC_SHADOW_THRESHOLD: u32 = 80_000;

/// Spawns a background thread that draws a shadow shape into a fresh pixmap and Gaussian-blurs it, then sends the finished pixmap back. The receiver is polled by the main thread each frame. `draw_fn` must be `Send + 'static`; it may only capture `Copy`/`Send` data (rect dimensions, colors, paths, alpha buffers).
pub(crate) fn spawn_shadow_async(
    tmp_w: u32,
    tmp_h: u32,
    blur_radius: f32,
    draw_fn: impl FnOnce(&mut tiny_skia::Pixmap) + Send + 'static,
) -> std::sync::mpsc::Receiver<tiny_skia::Pixmap> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut pixmap = match tiny_skia::Pixmap::new(tmp_w, tmp_h) {
            Some(p) => p,
            None => return,
        };
        draw_fn(&mut pixmap);
        if blur_radius > 0.0 {
            let sigma = renderer_core::blur_sigma(blur_radius);
            // The worker owns its scratch buffer; the main thread's is not shared across threads.
            let mut scratch = vec![0u8; pixmap.data().len()];
            let (w, h) = (pixmap.width(), pixmap.height());
            gaussian_blur(pixmap.data_mut(), w, h, sigma, &mut scratch);
        }
        let _ = tx.send(pixmap);
    });
    rx
}

/// Like `blit_cached_shadow`, but offloads computation of large shadows to a background thread. On a cache miss for a shadow whose pixmap exceeds `ASYNC_SHADOW_THRESHOLD`, the work is spawned (or, if already spawned, its result is polled) and the result lands 1-2 frames later. Small shadows fall back to the synchronous path.
///
/// While a blur is pending, the **previous shadow of the same size** is drawn in its place (`recent`). Without that stand-in the shadow blinks out for those frames: a desktop clock re-keys its shadow every minute (the text is part of the key) and its pixmap lands just past the threshold, so it took the async path and left a hole under the glyphs once a minute. A stand-in whose silhouette is one glyph stale for ~30 ms is not visible; its absence is.
///
/// `make_draw` yields the shape-drawing closure and its `Send + 'static` twin for the worker, and is called only on the paths that actually need one — never on a cache hit, and never while a worker is already producing the same pixmap. Producing them lazily is what lets a caller put expensive work (rasterizing a string to an alpha mask) behind the cache lookup instead of ahead of it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn blit_cached_shadow_async<K, D, A>(
    pixmap: &mut tiny_skia::Pixmap,
    cache: &mut renderer_cache::Cache<K, tiny_skia::Pixmap>,
    pending: &mut std::collections::HashMap<K, std::sync::mpsc::Receiver<tiny_skia::Pixmap>>,
    recent: &mut Option<(K, u32, u32)>,
    key: K,
    blit_x: i32,
    blit_y: i32,
    tmp_w: u32,
    tmp_h: u32,
    blur_radius: f32,
    blur_scratch: &mut Vec<u8>,
    transform: tiny_skia::Transform,
    clip: Option<&tiny_skia::Mask>,
    make_draw: impl FnOnce() -> (D, A),
) where
    K: std::hash::Hash + Eq + Clone,
    D: FnOnce(&mut tiny_skia::Pixmap),
    A: FnOnce(&mut tiny_skia::Pixmap) + Send + 'static,
{
    // For small shadows the spawn and channel overhead outweighs the blur cost.
    if tmp_w.saturating_mul(tmp_h) <= ASYNC_SHADOW_THRESHOLD {
        blit_cached_shadow(
            pixmap,
            cache,
            key,
            blit_x,
            blit_y,
            tmp_w,
            tmp_h,
            blur_radius,
            blur_scratch,
            transform,
            clip,
            || make_draw().0,
        );
        return;
    }

    if !cache.contains(&key) {
        // Either a worker is already computing it, or one needs spawning.
        if let Some(rx) = pending.get(&key) {
            match rx.try_recv() {
                Ok(tmp) => {
                    cache.insert(key.clone(), tmp);
                    pending.remove(&key);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // The worker failed, so drop the entry and let a later frame retry.
                    pending.remove(&key);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        } else {
            let rx = spawn_shadow_async(tmp_w, tmp_h, blur_radius, make_draw().1);
            pending.insert(key.clone(), rx);
        }
    }

    // A matching size is what makes the stand-in safe to blit at these coordinates.
    let drawn = if cache.contains(&key) {
        Some(key.clone())
    } else {
        recent
            .as_ref()
            .filter(|(_, w, h)| *w == tmp_w && *h == tmp_h)
            .map(|(k, _, _)| k.clone())
    };
    let Some(draw_key) = drawn else {
        return;
    };
    if let Some(cached) = cache.get(&draw_key) {
        pixmap.draw_pixmap(
            blit_x,
            blit_y,
            cached.as_ref(),
            &tiny_skia::PixmapPaint {
                blend_mode: tiny_skia::BlendMode::SourceOver,
                ..Default::default()
            },
            transform,
            clip,
        );
        // Only a real hit updates the stand-in, or a stale one would keep re-electing itself.
        if draw_key == key {
            *recent = Some((key, tmp_w, tmp_h));
        }
    }
}

pub(crate) fn blit_cached_shadow<K, D>(
    pixmap: &mut tiny_skia::Pixmap,
    cache: &mut renderer_cache::Cache<K, tiny_skia::Pixmap>,
    key: K,
    blit_x: i32,
    blit_y: i32,
    tmp_w: u32,
    tmp_h: u32,
    blur_radius: f32,
    blur_scratch: &mut Vec<u8>,
    transform: tiny_skia::Transform,
    clip: Option<&tiny_skia::Mask>,
    make_draw: impl FnOnce() -> D,
) where
    K: std::hash::Hash + Eq + Clone,
    D: FnOnce(&mut tiny_skia::Pixmap),
{
    let paint = tiny_skia::PixmapPaint {
        blend_mode: tiny_skia::BlendMode::SourceOver,
        ..Default::default()
    };
    if let Some(cached) = cache.get(&key) {
        pixmap.draw_pixmap(blit_x, blit_y, cached.as_ref(), &paint, transform, clip);
        return;
    }
    // A shadow too large for the budget still draws rather than silently vanishing.
    if let Some(tmp) = render_shadow_pixmap(tmp_w, tmp_h, blur_radius, blur_scratch, make_draw()) {
        pixmap.draw_pixmap(blit_x, blit_y, tmp.as_ref(), &paint, transform, clip);
        cache.insert(key, tmp);
    }
}

pub(crate) fn gaussian_blur(
    data: &mut [u8],
    width: u32,
    height: u32,
    sigma: f32,
    scratch: &mut Vec<u8>,
) {
    if sigma < 0.5 || width == 0 || height == 0 {
        return;
    }
    let r = ((sigma * 1.5).round() as u32).max(1);
    scratch.resize(data.len(), 0);
    let w = width as usize;
    let h = height as usize;
    for _ in 0..3 {
        box_blur_h(data, width, height, r, scratch);
        // Transposed so both passes stay cache-sequential: transpose into scratch, blur its rows (the original columns), then transpose back. `data` is safe as the inner scratch, having already been copied out.
        transpose_to_scratch(data, scratch, w, h);
        box_blur_h(scratch, height, width, r, data);
        transpose_to_scratch(scratch, data, h, w);
    }
}

fn transpose_to_scratch(src: &[u8], dst: &mut [u8], width: usize, height: usize) {
    const BLOCK: usize = 8;
    let src_stride = width * 4;
    let dst_stride = height * 4; // transposed: dst is height×width, so each row is `height` pixels
    let mut block_y = 0;
    while block_y < height {
        let by_end = (block_y + BLOCK).min(height);
        let mut block_x = 0;
        while block_x < width {
            let bx_end = (block_x + BLOCK).min(width);
            for y in block_y..by_end {
                for x in block_x..bx_end {
                    let src_idx = y * src_stride + x * 4;
                    let dst_idx = x * dst_stride + y * 4;
                    dst[dst_idx..dst_idx + 4].copy_from_slice(&src[src_idx..src_idx + 4]);
                }
            }
            block_x += BLOCK;
        }
        block_y += BLOCK;
    }
}

fn box_blur_h(data: &mut [u8], width: u32, height: u32, r: u32, scratch: &mut [u8]) {
    let w = width as usize;
    let h = height as usize;
    let r = r as usize;
    if w == 0 || h == 0 {
        return;
    }
    let full_count = (2 * r + 1) as u32;
    let recip: u32 = ((1u32 << 16) + full_count - 1) / full_count;
    let row_size = w * 4;
    use rayon::prelude::*;
    data.par_chunks_mut(row_size)
        .zip(scratch.par_chunks_mut(row_size))
        .for_each(|(row_data, row_scratch)| {
            let initial_end = (r + 1).min(w);
            // All four channels together, so each row is walked once instead of four times.
            let mut sum = [0u32; 4];
            for xi in 0..initial_end {
                let base = xi * 4;
                for c in 0..4 {
                    sum[c] += row_data[base + c] as u32;
                }
            }
            let mut count = initial_end as u32;
            for x in 0..w {
                let out_base = x * 4;
                for c in 0..4 {
                    row_scratch[out_base + c] = (if count == full_count {
                        (sum[c] * recip) >> 16
                    } else {
                        sum[c] / count
                    }) as u8;
                }
                if x + r + 1 < w {
                    let add_base = (x + r + 1) * 4;
                    for c in 0..4 {
                        sum[c] += row_data[add_base + c] as u32;
                    }
                    count += 1;
                }
                if x >= r {
                    let sub_base = (x - r) * 4;
                    for c in 0..4 {
                        sum[c] -= row_data[sub_base + c] as u32;
                    }
                    count -= 1;
                }
            }
            row_data.copy_from_slice(row_scratch);
        });
}
