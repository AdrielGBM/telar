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
    for _ in 0..3 {
        box_blur_h(data, width, height, r, scratch);
        box_blur_v(data, width, height, r, scratch);
    }
}

fn box_blur_h(data: &mut [u8], width: u32, height: u32, r: u32, scratch: &mut Vec<u8>) {
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
