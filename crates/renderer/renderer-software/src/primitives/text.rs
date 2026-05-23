use geometry_core::Rect;
use renderer_core::{Color, TextStyle};

fn tint_premultiplied(pixels: &mut [u8], color: Color) {
    let [r, g, b, a] = color.to_rgba8();
    for chunk in pixels.chunks_exact_mut(4) {
        chunk[0] = ((chunk[0] as u32 * r as u32) / 255) as u8;
        chunk[1] = ((chunk[1] as u32 * g as u32) / 255) as u8;
        chunk[2] = ((chunk[2] as u32 * b as u32) / 255) as u8;
        chunk[3] = ((chunk[3] as u32 * a as u32) / 255) as u8;
    }
}

fn overlaps_clip(x: f32, y: f32, w: f32, h: f32, clip: Option<Rect>) -> bool {
    let Some(clip) = clip else { return true };
    x + w > clip.x && y + h > clip.y && x < clip.x + clip.width && y < clip.y + clip.height
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
) {
    if let Some(shadow) = style.shadow {
        let (arc, tex_w, tex_h) = shaper.rasterize_alpha(text, rect, style);
        if tex_w > 0 && tex_h > 0 {
            let sigma = shadow.blur_radius / 2.0;
            let padding = (sigma * 3.0).ceil() as i32 + 1;

            // Guard the expensive shadow blur path: skip when the shadow's pixel-space bounds are fully outside the current clip rect. Text glyphs are still drawn below if the body rect is visible.
            let shadow_x = rect.x + shadow.offset_x - padding as f32;
            let shadow_y = rect.y + shadow.offset_y - padding as f32;
            let shadow_w = tex_w as f32 + 2.0 * padding as f32 + 2.0;
            let shadow_h = tex_h as f32 + 2.0 * padding as f32 + 2.0;
            if overlaps_clip(shadow_x, shadow_y, shadow_w, shadow_h, current_clip_rect) {
                let mut shadow_pixels = arc.to_vec();
                tint_premultiplied(&mut shadow_pixels, shadow.color);

                let tmp_w = tex_w + 2 * padding as u32 + 2;
                let tmp_h = tex_h + 2 * padding as u32 + 2;
                if let Some(mut tmp) = tiny_skia::Pixmap::new(tmp_w, tmp_h) {
                    if let Some(size) = tiny_skia::IntSize::from_wh(tex_w, tex_h) {
                        if let Some(src) = tiny_skia::Pixmap::from_vec(shadow_pixels, size) {
                            tmp.draw_pixmap(
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
                    if sigma >= 0.5 {
                        crate::primitives::gaussian_blur(
                            tmp.data_mut(),
                            tmp_w,
                            tmp_h,
                            sigma,
                            blur_scratch,
                        );
                    }
                    pixmap.draw_pixmap(
                        rect.x as i32 + shadow.offset_x as i32 - padding,
                        rect.y as i32 + shadow.offset_y as i32 - padding,
                        tmp.as_ref(),
                        &tiny_skia::PixmapPaint {
                            blend_mode: tiny_skia::BlendMode::SourceOver,
                            ..Default::default()
                        },
                        transform,
                        clip,
                    );
                }
            }

            let mut text_pixels = arc.to_vec();
            tint_premultiplied(&mut text_pixels, style.color);

            let Some(size) = tiny_skia::IntSize::from_wh(tex_w, tex_h) else {
                return;
            };
            if let Some(src) = tiny_skia::Pixmap::from_vec(text_pixels, size) {
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
        }
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
}
