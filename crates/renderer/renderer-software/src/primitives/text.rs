use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;

use clru::{CLruCache, CLruCacheConfig};
use geometry_core::Rect;
use renderer_core::{Color, TextStyle};
use rustc_hash::{FxBuildHasher, FxHasher};

fn tint_premultiplied(pixels: &mut [u8], color: Color) {
    let [r, g, b, a] = color.to_rgba8();
    for chunk in pixels.chunks_exact_mut(4) {
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
    pub tex_w: u32,
    pub tex_h: u32,
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
) {
    if let Some(shadow) = style.shadow {
        let (arc, tex_w, tex_h) = shaper.rasterize_alpha(text, rect, style);
        if tex_w > 0 && tex_h > 0 {
            let sigma = shadow.blur_radius / 2.0;
            let padding = (sigma * 3.0).ceil() as i32 + 1;

            let shadow_x = rect.x + shadow.offset_x - padding as f32;
            let shadow_y = rect.y + shadow.offset_y - padding as f32;
            let shadow_w = tex_w as f32 + 2.0 * padding as f32 + 2.0;
            let shadow_h = tex_h as f32 + 2.0 * padding as f32 + 2.0;
            if renderer_core::culling::overlaps(
                shadow_x,
                shadow_y,
                shadow_w,
                shadow_h,
                current_clip_rect,
            ) {
                let [sr, sg, sb, sa] = shadow.color.to_rgba8();
                let shadow_key = TextShadowCacheKey {
                    text_hash: hash_text(text),
                    font_size_bits: style.font_size.to_bits(),
                    tex_w,
                    tex_h,
                    shadow_color: u32::from_le_bytes([sr, sg, sb, sa]),
                    blur_radius_bits: shadow.blur_radius.to_bits(),
                };

                let tmp_w = tex_w + 2 * padding as u32 + 2;
                let tmp_h = tex_h + 2 * padding as u32 + 2;
                let shadow_color = shadow.color;

                crate::primitives::blit_cached_shadow(
                    pixmap,
                    text_shadow_cache,
                    shadow_key,
                    rect.x as i32 + shadow.offset_x as i32 - padding,
                    rect.y as i32 + shadow.offset_y as i32 - padding,
                    tmp_w,
                    tmp_h,
                    shadow.blur_radius,
                    blur_scratch,
                    transform,
                    clip,
                    |tmp_pmap| {
                        let mut shadow_pixels = arc.to_vec();
                        tint_premultiplied(&mut shadow_pixels, shadow_color);
                        if let Some(size) = tiny_skia::IntSize::from_wh(tex_w, tex_h) {
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
                    },
                );
            }

            let body_key = renderer_text::make_text_cache_key(
                text,
                style.font_size,
                tex_w,
                tex_h,
                style.color,
            );
            let paint = tiny_skia::PixmapPaint {
                blend_mode: tiny_skia::BlendMode::SourceOver,
                ..Default::default()
            };
            if text_pixmap_cache.get(&body_key).is_none() {
                let mut body_pixels = arc.to_vec();
                tint_premultiplied(&mut body_pixels, style.color);
                if let Some(size) = tiny_skia::IntSize::from_wh(tex_w, tex_h) {
                    if let Some(src) = tiny_skia::Pixmap::from_vec(body_pixels, size) {
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
        style.color,
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
        return;
    }

    // Use rasterize_alpha so the shaper's alpha cache is keyed without color — color changes tint
    // the cached alpha pixmap instead of triggering a full re-rasterize.
    let (pixels_arc, w, h) = shaper.rasterize_alpha(text, rect, style);
    if w == 0 || h == 0 {
        return;
    }
    let Some(size) = tiny_skia::IntSize::from_wh(w, h) else {
        return;
    };
    let mut tinted = pixels_arc.to_vec();
    tint_premultiplied(&mut tinted, style.color);
    let Some(src) = tiny_skia::Pixmap::from_vec(tinted, size) else {
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
}
