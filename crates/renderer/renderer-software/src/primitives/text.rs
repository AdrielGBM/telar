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

pub(crate) fn draw_text(
    pixmap: &mut tiny_skia::Pixmap,
    shaper: &mut renderer_text::TextShaper,
    text: &str,
    rect: Rect,
    style: &TextStyle,
    transform: tiny_skia::Transform,
    clip: Option<&tiny_skia::Mask>,
    blur_scratch: &mut Vec<u8>,
) {
    if let Some(shadow) = style.shadow {
        let (arc, tex_w, tex_h) = shaper.rasterize_alpha(text, rect, style);
        if tex_w > 0 && tex_h > 0 {
            let mut shadow_pixels = arc.to_vec();
            tint_premultiplied(&mut shadow_pixels, shadow.color);

            let sigma = shadow.blur_radius / 2.0;
            let padding = (sigma * 3.0).ceil() as i32 + 1;
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

    let (pixels_arc, tex_width, tex_height) = shaper.rasterize(text, rect, style);
    if tex_width == 0 || tex_height == 0 {
        return;
    }

    let Some(size) = tiny_skia::IntSize::from_wh(tex_width, tex_height) else {
        return;
    };

    if let Some(src) = tiny_skia::Pixmap::from_vec(pixels_arc.to_vec(), size) {
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
