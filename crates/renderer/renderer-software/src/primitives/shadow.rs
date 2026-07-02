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
            // The worker owns its own scratch buffer; the main thread's blur_scratch is not shared across threads.
            let mut scratch = vec![0u8; pixmap.data().len()];
            let (w, h) = (pixmap.width(), pixmap.height());
            gaussian_blur(pixmap.data_mut(), w, h, sigma, &mut scratch);
        }
        let _ = tx.send(pixmap);
    });
    rx
}

/// Like `blit_cached_shadow`, but offloads computation of large shadows to a background thread. On a cache miss for a shadow whose pixmap exceeds `ASYNC_SHADOW_THRESHOLD`, the work is spawned (or, if already spawned, its result is polled); the shadow is simply not drawn this frame and appears 1-2 frames later once the worker finishes and the result is cached. Small shadows fall back to the synchronous path. `draw_async_fn` is the `Send + 'static` variant of `draw_fn` used for the worker thread.
#[allow(clippy::too_many_arguments)]
pub(crate) fn blit_cached_shadow_async<K, H, S>(
    pixmap: &mut tiny_skia::Pixmap,
    cache: &mut clru::CLruCache<K, tiny_skia::Pixmap, H, S>,
    pending: &mut std::collections::HashMap<K, std::sync::mpsc::Receiver<tiny_skia::Pixmap>>,
    key: K,
    blit_x: i32,
    blit_y: i32,
    tmp_w: u32,
    tmp_h: u32,
    blur_radius: f32,
    blur_scratch: &mut Vec<u8>,
    transform: tiny_skia::Transform,
    clip: Option<&tiny_skia::Mask>,
    draw_fn: impl FnOnce(&mut tiny_skia::Pixmap),
    draw_async_fn: impl FnOnce(&mut tiny_skia::Pixmap) + Send + 'static,
) where
    K: std::hash::Hash + Eq + Clone,
    H: std::hash::BuildHasher,
    S: clru::WeightScale<K, tiny_skia::Pixmap>,
{
    // Small shadows: the spawn/channel overhead outweighs the blur cost, so compute inline.
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
            draw_fn,
        );
        return;
    }

    if cache.get(&key).is_none() {
        // Not cached yet. Either a worker is already computing it (poll for completion) or we need to spawn one.
        if let Some(rx) = pending.get(&key) {
            match rx.try_recv() {
                Ok(tmp) => {
                    cache.put_with_weight(key.clone(), tmp).ok();
                    pending.remove(&key);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // Worker failed (e.g. allocation failure); drop the entry so a later frame can retry.
                    pending.remove(&key);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        } else {
            let rx = spawn_shadow_async(tmp_w, tmp_h, blur_radius, draw_async_fn);
            pending.insert(key.clone(), rx);
        }
    }

    // Render the shadow this frame only if it is already cached; otherwise it appears once the worker result is inserted on a later frame.
    if let Some(cached) = cache.get(&key) {
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
    }
}

pub(crate) fn blit_cached_shadow<K, H, S>(
    pixmap: &mut tiny_skia::Pixmap,
    cache: &mut clru::CLruCache<K, tiny_skia::Pixmap, H, S>,
    key: K,
    blit_x: i32,
    blit_y: i32,
    tmp_w: u32,
    tmp_h: u32,
    blur_radius: f32,
    blur_scratch: &mut Vec<u8>,
    transform: tiny_skia::Transform,
    clip: Option<&tiny_skia::Mask>,
    draw_fn: impl FnOnce(&mut tiny_skia::Pixmap),
) where
    K: std::hash::Hash + Eq + Clone,
    H: std::hash::BuildHasher,
    S: clru::WeightScale<K, tiny_skia::Pixmap>,
{
    if cache.get(&key).is_none() {
        if let Some(tmp) = render_shadow_pixmap(tmp_w, tmp_h, blur_radius, blur_scratch, draw_fn) {
            cache.put_with_weight(key.clone(), tmp).ok();
        }
    }
    if let Some(cached) = cache.get(&key) {
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
        // Vertical pass via transpose to keep both passes cache-sequential. Transpose data(w×h) into scratch(h×w), blur scratch's rows (which are original columns), then transpose back. data is safe to use as the inner scratch because we've already copied data into scratch before calling box_blur_h on scratch.
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
            // Accumulate all 4 channels together so each row is walked once instead of 4 times.
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
