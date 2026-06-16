pub(crate) mod colr;
pub(crate) mod image;
pub(crate) mod line;
pub(crate) mod path;
pub(crate) mod rect;
pub(crate) mod text;

#[inline(always)]
pub(crate) fn to_skia_color(color: renderer_core::Color) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba(
        color.r.clamp(0.0, 1.0),
        color.g.clamp(0.0, 1.0),
        color.b.clamp(0.0, 1.0),
        color.a.clamp(0.0, 1.0),
    )
    .expect("channels clamped to [0,1]")
}

pub(crate) fn to_skia_line_cap(cap: renderer_core::LineCap) -> tiny_skia::LineCap {
    match cap {
        renderer_core::LineCap::Butt => tiny_skia::LineCap::Butt,
        renderer_core::LineCap::Round => tiny_skia::LineCap::Round,
        renderer_core::LineCap::Square => tiny_skia::LineCap::Square,
    }
}

pub(crate) fn to_skia_line_join(join: renderer_core::LineJoin) -> tiny_skia::LineJoin {
    match join {
        renderer_core::LineJoin::Miter => tiny_skia::LineJoin::Miter,
        renderer_core::LineJoin::Round => tiny_skia::LineJoin::Round,
        renderer_core::LineJoin::Bevel => tiny_skia::LineJoin::Bevel,
    }
}

#[inline]
pub(crate) fn fill_to_paint(fill: renderer_core::Paint) -> tiny_skia::Paint<'static> {
    let mut paint = tiny_skia::Paint::default();
    paint.anti_alias = true;
    match fill {
        renderer_core::Paint::Solid(c) => {
            paint.set_color(to_skia_color(c));
        }
        renderer_core::Paint::Gradient(g) => {
            let mut skia_stops = Vec::with_capacity(8);
            skia_stops.extend(
                g.stops
                    .active()
                    .iter()
                    .map(|s| tiny_skia::GradientStop::new(s.position, to_skia_color(s.color))),
            );
            match g.kind {
                renderer_core::GradientKind::Linear { start, end } => {
                    if let Some(shader) = tiny_skia::LinearGradient::new(
                        tiny_skia::Point::from_xy(start.x, start.y),
                        tiny_skia::Point::from_xy(end.x, end.y),
                        skia_stops,
                        tiny_skia::SpreadMode::Pad,
                        tiny_skia::Transform::identity(),
                    ) {
                        paint.shader = shader;
                    }
                }
                renderer_core::GradientKind::Radial { center, radius } => {
                    if let Some(shader) = tiny_skia::RadialGradient::new(
                        tiny_skia::Point::from_xy(center.x, center.y),
                        0.0,
                        tiny_skia::Point::from_xy(center.x, center.y),
                        radius,
                        skia_stops,
                        tiny_skia::SpreadMode::Pad,
                        tiny_skia::Transform::identity(),
                    ) {
                        paint.shader = shader;
                    }
                }
            }
        }
    }
    paint
}

/// Creates a temporary pixmap of the given size, calls `draw_fn` to draw the shadow shape,
/// then Gaussian-blurs the result. Returns None if pixmap allocation fails.
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

/// Ensures a shadow pixmap is in `cache` under `key`. If absent, renders it via `draw_fn`.
/// Then blits it onto `pixmap` at `(blit_x, blit_y)`.
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
        // Vertical pass via transpose to keep both passes cache-sequential.
        // Transpose data(w×h) into scratch(h×w), blur scratch's rows (which are original columns),
        // then transpose back. data is safe to use as the inner scratch because we've already
        // copied data into scratch before calling box_blur_h on scratch.
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

#[allow(dead_code)]
fn box_blur_v(data: &mut [u8], width: u32, height: u32, r: u32, scratch: &mut Vec<u8>) {
    let w = width as usize;
    let h = height as usize;
    let r = r as usize;
    if w == 0 || h == 0 {
        return;
    }
    let full_count = (2 * r + 1) as u32;
    let recip: u32 = ((1u32 << 16) + full_count - 1) / full_count;
    for x in 0..w {
        let initial_end = (r + 1).min(h);
        // Accumulate all 4 channels together so each column is walked once instead of 4 times.
        let mut sum = [0u32; 4];
        for yi in 0..initial_end {
            let base = (yi * w + x) * 4;
            for c in 0..4 {
                sum[c] += data[base + c] as u32;
            }
        }
        let mut count = initial_end as u32;
        for y in 0..h {
            let out_base = (y * w + x) * 4;
            for c in 0..4 {
                scratch[out_base + c] = (if count == full_count {
                    (sum[c] * recip) >> 16
                } else {
                    sum[c] / count
                }) as u8;
            }
            if y + r + 1 < h {
                let add_base = ((y + r + 1) * w + x) * 4;
                for c in 0..4 {
                    sum[c] += data[add_base + c] as u32;
                }
                count += 1;
            }
            if y >= r {
                let sub_base = ((y - r) * w + x) * 4;
                for c in 0..4 {
                    sum[c] -= data[sub_base + c] as u32;
                }
                count -= 1;
            }
        }
    }
    data.copy_from_slice(&scratch[..data.len()]);
}
