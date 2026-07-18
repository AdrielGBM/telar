use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;

use clru::{CLruCache, CLruCacheConfig};
use geometry_core::Rect;
use renderer_core::{Color, TextStyle};
use rustc_hash::{FxBuildHasher, FxHasher};

fn tint_premultiplied(pixels: &mut [u8], color: Color) {
    use wide::u32x8;
    let [r, g, b, a] = color.to_rgba8();
    let n_simd = pixels.len() / 32;
    let (simd_pixels, rest) = pixels.split_at_mut(n_simd * 32);

    // fast_div255(v, c) = (v * c + 128) >> 8, a 1-ulp approximation of v*c/255
    let r_splat = u32x8::splat(r as u32);
    let g_splat = u32x8::splat(g as u32);
    let b_splat = u32x8::splat(b as u32);
    let a_splat = u32x8::splat(a as u32);
    let bias = u32x8::splat(128);

    for chunk in simd_pixels.chunks_exact_mut(32) {
        let rv = u32x8::from([
            chunk[0] as u32,
            chunk[4] as u32,
            chunk[8] as u32,
            chunk[12] as u32,
            chunk[16] as u32,
            chunk[20] as u32,
            chunk[24] as u32,
            chunk[28] as u32,
        ]);
        let gv = u32x8::from([
            chunk[1] as u32,
            chunk[5] as u32,
            chunk[9] as u32,
            chunk[13] as u32,
            chunk[17] as u32,
            chunk[21] as u32,
            chunk[25] as u32,
            chunk[29] as u32,
        ]);
        let bv = u32x8::from([
            chunk[2] as u32,
            chunk[6] as u32,
            chunk[10] as u32,
            chunk[14] as u32,
            chunk[18] as u32,
            chunk[22] as u32,
            chunk[26] as u32,
            chunk[30] as u32,
        ]);
        let av = u32x8::from([
            chunk[3] as u32,
            chunk[7] as u32,
            chunk[11] as u32,
            chunk[15] as u32,
            chunk[19] as u32,
            chunk[23] as u32,
            chunk[27] as u32,
            chunk[31] as u32,
        ]);

        let rv: [u32; 8] = ((rv * r_splat + bias) >> u32x8::splat(8)).into();
        let gv: [u32; 8] = ((gv * g_splat + bias) >> u32x8::splat(8)).into();
        let bv: [u32; 8] = ((bv * b_splat + bias) >> u32x8::splat(8)).into();
        let av: [u32; 8] = ((av * a_splat + bias) >> u32x8::splat(8)).into();

        for i in 0..8usize {
            chunk[i * 4] = rv[i] as u8;
            chunk[i * 4 + 1] = gv[i] as u8;
            chunk[i * 4 + 2] = bv[i] as u8;
            chunk[i * 4 + 3] = av[i] as u8;
        }
    }

    // Scalar fallback for remaining < 8 pixels
    for chunk in rest.chunks_exact_mut(4) {
        chunk[0] = ((chunk[0] as u32 * r as u32) / 255) as u8;
        chunk[1] = ((chunk[1] as u32 * g as u32) / 255) as u8;
        chunk[2] = ((chunk[2] as u32 * b as u32) / 255) as u8;
        chunk[3] = ((chunk[3] as u32 * a as u32) / 255) as u8;
    }
}

fn hash_text(text: &str) -> u64 {
    let mut h = FxHasher::default();
    text.hash(&mut h);
    h.finish()
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TextShadowCacheKey {
    pub text_hash: u64,
    pub font_size_bits: u32,
    pub texture_width: u32,
    pub texture_height: u32,
    pub shadow_color: u32,
    pub blur_radius_bits: u32,
}

pub(crate) type TextShadowCache = CLruCache<
    TextShadowCacheKey,
    tiny_skia::Pixmap,
    FxBuildHasher,
    crate::primitives::image::PixmapByteScale,
>;

pub(crate) fn new_text_shadow_cache(budget_bytes: usize) -> TextShadowCache {
    CLruCache::with_config(
        CLruCacheConfig::new(NonZeroUsize::new(budget_bytes.max(1)).unwrap())
            .with_hasher(FxBuildHasher::default())
            .with_scale(crate::primitives::image::PixmapByteScale),
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_text(
    pixmap: &mut tiny_skia::Pixmap,
    shaper: &mut renderer_text::TextShaper,
    text: &str,
    rect: Rect,
    style: &TextStyle,
    transform: tiny_skia::Transform,
    clip: Option<&tiny_skia::Mask>,
    current_clip_rect: Option<Rect>,
    blur_scratch: &mut Vec<u8>,
    text_pixmap_cache: &mut lru::LruCache<renderer_text::TextCacheKey, tiny_skia::Pixmap>,
    text_shadow_cache: &mut TextShadowCache,
    pending_text_shadows: &mut std::collections::HashMap<
        TextShadowCacheKey,
        std::sync::mpsc::Receiver<tiny_skia::Pixmap>,
    >,
) {
    if let Some(shadow) = style.shadow {
        let (arc, texture_width, texture_height) = shaper.rasterize_alpha(text, rect, style);
        if texture_width > 0 && texture_height > 0 {
            let shadow_layout = renderer_core::ShadowLayout::compute(
                shadow.blur_radius,
                rect.x + shadow.offset_x,
                rect.x + shadow.offset_x + texture_width as f32,
                rect.y + shadow.offset_y,
                rect.y + shadow.offset_y + texture_height as f32,
                1.0,
            );
            let padding = shadow_layout.padding;

            let shadow_x = rect.x + shadow.offset_x - padding as f32;
            let shadow_y = rect.y + shadow.offset_y - padding as f32;
            let shadow_w = texture_width as f32 + 2.0 * padding as f32 + 2.0;
            let shadow_h = texture_height as f32 + 2.0 * padding as f32 + 2.0;
            if renderer_core::culling::overlaps(
                shadow_x + transform.tx,
                shadow_y + transform.ty,
                shadow_w,
                shadow_h,
                current_clip_rect,
            ) {
                let q_blur = crate::primitives::quantize_blur(shadow.blur_radius);
                let [sr, sg, sb, sa] = shadow.color.to_rgba8();
                let shadow_key = TextShadowCacheKey {
                    text_hash: hash_text(text),
                    font_size_bits: style.font_size.to_bits(),
                    texture_width,
                    texture_height,
                    shadow_color: u32::from_le_bytes([sr, sg, sb, sa]),
                    blur_radius_bits: q_blur.to_bits(),
                };

                let tmp_w = texture_width + 2 * padding as u32 + 2;
                let tmp_h = texture_height + 2 * padding as u32 + 2;
                let shadow_color = shadow.color;

                // The shadow shape is the tinted alpha texture; it only needs the (Send) alpha buffer and Copy params, so large text shadows can be blurred on a background thread. The async closure owns a clone of the alpha Arc.
                let draw_text_shadow = move |tmp_pmap: &mut tiny_skia::Pixmap, alpha: &[u8]| {
                    let mut shadow_pixels = alpha.to_vec();
                    tint_premultiplied(&mut shadow_pixels, shadow_color);
                    if let Some(size) = tiny_skia::IntSize::from_wh(texture_width, texture_height) {
                        if let Some(src) = tiny_skia::Pixmap::from_vec(shadow_pixels, size) {
                            tmp_pmap.draw_pixmap(
                                padding,
                                padding,
                                src.as_ref(),
                                &tiny_skia::PixmapPaint {
                                    blend_mode: tiny_skia::BlendMode::SourceOver,
                                    ..Default::default()
                                },
                                tiny_skia::Transform::identity(),
                                None,
                            );
                        }
                    }
                };

                let async_alpha = arc.clone();
                let draw_async = move |tmp_pmap: &mut tiny_skia::Pixmap| {
                    draw_text_shadow(tmp_pmap, &async_alpha);
                };

                crate::primitives::blit_cached_shadow_async(
                    pixmap,
                    text_shadow_cache,
                    pending_text_shadows,
                    shadow_key,
                    rect.x as i32 + shadow.offset_x as i32 - padding,
                    rect.y as i32 + shadow.offset_y as i32 - padding,
                    tmp_w,
                    tmp_h,
                    q_blur,
                    blur_scratch,
                    transform,
                    clip,
                    |tmp_pmap| draw_text_shadow(tmp_pmap, &arc),
                    draw_async,
                );
            }

            let body_key = renderer_text::make_text_cache_key(
                text,
                style.font_size,
                texture_width,
                texture_height,
                style.paint.solid_color(),
                renderer_text::text_style_bits(style),
            );
            let paint = tiny_skia::PixmapPaint {
                blend_mode: tiny_skia::BlendMode::SourceOver,
                ..Default::default()
            };
            if text_pixmap_cache.get(&body_key).is_none() {
                // Use rasterize (not rasterize_alpha+tint) so color emoji preserve their natural colors instead of being multiplied by the text color.
                let (colored_arc, _, _) = shaper.rasterize(text, rect, style);
                if let Some(size) = tiny_skia::IntSize::from_wh(texture_width, texture_height) {
                    if let Some(src) = tiny_skia::Pixmap::from_vec(colored_arc.to_vec(), size) {
                        text_pixmap_cache.put(body_key.clone(), src);
                    }
                }
            }
            if renderer_core::culling::overlaps(
                rect.x + transform.tx,
                rect.y + transform.ty,
                rect.width,
                rect.height,
                current_clip_rect,
            ) {
                if let Some(src) = text_pixmap_cache.get(&body_key) {
                    pixmap.draw_pixmap(
                        rect.x as i32,
                        rect.y as i32,
                        src.as_ref(),
                        &paint,
                        transform,
                        clip,
                    );
                }
            }
        }

        draw_colr_fallback(pixmap, shaper, text, rect, style, transform, clip);
        return;
    }

    if !renderer_core::culling::overlaps(
        rect.x + transform.tx,
        rect.y + transform.ty,
        rect.width,
        rect.height,
        current_clip_rect,
    ) {
        return;
    }

    let tex_width = rect.width.ceil() as u32;
    let tex_height = rect.height.ceil() as u32;
    if tex_width == 0 || tex_height == 0 {
        return;
    }
    let cache_key = renderer_text::make_text_cache_key(
        text,
        style.font_size,
        tex_width,
        tex_height,
        style.paint.solid_color(),
        renderer_text::text_style_bits(style),
    );

    let paint = tiny_skia::PixmapPaint {
        blend_mode: tiny_skia::BlendMode::SourceOver,
        ..Default::default()
    };

    if let Some(src) = text_pixmap_cache.get(&cache_key) {
        pixmap.draw_pixmap(
            rect.x as i32,
            rect.y as i32,
            src.as_ref(),
            &paint,
            transform,
            clip,
        );
        draw_colr_fallback(pixmap, shaper, text, rect, style, transform, clip);
        return;
    }

    // Use rasterize (not rasterize_alpha+tint) so color emoji preserve their natural colors instead of being multiplied by the text color.
    let (pixels_arc, w, h) = shaper.rasterize(text, rect, style);
    if w == 0 || h == 0 {
        return;
    }
    let Some(size) = tiny_skia::IntSize::from_wh(w, h) else {
        return;
    };
    let Some(src) = tiny_skia::Pixmap::from_vec(pixels_arc.to_vec(), size) else {
        return;
    };
    pixmap.draw_pixmap(
        rect.x as i32,
        rect.y as i32,
        src.as_ref(),
        &paint,
        transform,
        clip,
    );
    text_pixmap_cache.put(cache_key, src);

    draw_colr_fallback(pixmap, shaper, text, rect, style, transform, clip);
}

/// Draws a rich-text paragraph: rasterizes the styled runs to one colour block (each run in its own colour)
/// and blits it. No shadow or block cache — the rich path serves dynamic notification bodies, not hot UI text.
pub(crate) fn draw_rich_text(
    pixmap: &mut tiny_skia::Pixmap,
    shaper: &mut renderer_text::TextShaper,
    runs: &[renderer_core::TextRun],
    rect: Rect,
    base: &TextStyle,
    transform: tiny_skia::Transform,
    clip: Option<&tiny_skia::Mask>,
    current_clip_rect: Option<Rect>,
) {
    if !renderer_core::culling::overlaps(
        rect.x + transform.tx,
        rect.y + transform.ty,
        rect.width,
        rect.height,
        current_clip_rect,
    ) {
        return;
    }
    let (pixels_arc, w, h) = shaper.rasterize_rich(runs, rect, base);
    if w == 0 || h == 0 {
        return;
    }
    let Some(size) = tiny_skia::IntSize::from_wh(w, h) else {
        return;
    };
    let Some(src) = tiny_skia::Pixmap::from_vec(pixels_arc.to_vec(), size) else {
        return;
    };
    let paint = tiny_skia::PixmapPaint {
        blend_mode: tiny_skia::BlendMode::SourceOver,
        ..Default::default()
    };
    pixmap.draw_pixmap(
        rect.x as i32,
        rect.y as i32,
        src.as_ref(),
        &paint,
        transform,
        clip,
    );
}

/// Renders COLR v1 color glyphs that swash cannot rasterize. swash returns `None` for these glyphs,
/// so `Buffer::draw` omits them; we re-rasterize via skrifa + tiny-skia and blit them on top.
fn draw_colr_fallback(
    pixmap: &mut tiny_skia::Pixmap,
    shaper: &mut renderer_text::TextShaper,
    text: &str,
    rect: Rect,
    style: &TextStyle,
    transform: tiny_skia::Transform,
    clip: Option<&tiny_skia::Mask>,
) {
    let mut colr_glyphs: Vec<renderer_text::ColrGlyph> = Vec::new();
    shaper.collect_colr_glyphs(text, rect, style, &mut colr_glyphs);
    if !colr_glyphs.is_empty() {
        crate::primitives::colr::draw_colr_glyphs(pixmap, &colr_glyphs, shaper, transform, clip);
    }
}
