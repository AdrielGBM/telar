//! COLR v1 color-glyph rendering for the software renderer.
//!
//! swash 0.2.x returns `None` when rasterizing COLR v1 glyphs (commonly emoji, but COLR v1 also
//! covers colored icons and decorative glyphs; e.g. Android's NotoColorEmoji.ttf), so
//! `cosmic_text::Buffer::draw` silently drops them. We re-rasterize them with the shared
//! skrifa-based rasterizer in renderer-text and blit the result onto the framebuffer pixmap.

use renderer_text::colr::rasterize_colr_glyph;
use tiny_skia::{BlendMode, IntSize, Mask, Pixmap, PixmapPaint, Transform};

/// Renders COLR v1 color glyphs that swash could not rasterize, blitting them onto `pixmap`.
pub(crate) fn draw_colr_glyphs(
    pixmap: &mut Pixmap,
    glyphs: &[renderer_text::ColrGlyph],
    shaper: &mut renderer_text::TextShaper,
    outer_transform: Transform,
    outer_clip: Option<&Mask>,
) {
    use std::collections::HashMap;

    // Group glyphs by font so each font's bytes are loaded only once.
    let mut by_font: HashMap<renderer_text::fontdb::ID, Vec<&renderer_text::ColrGlyph>> =
        HashMap::new();
    for glyph in glyphs {
        by_font.entry(glyph.font_id).or_default().push(glyph);
    }

    for (font_id, font_glyphs) in by_font {
        // Cached per font id: reuses bytes already in memory instead of re-reading and copying the (often multi-MB) emoji font on every emoji draw.
        let Some(font) = shaper.colr_font_bytes(font_id) else {
            continue;
        };
        let (bytes, face_index) = (&font.0, font.1);

        for glyph in font_glyphs {
            // Software DrawCommands are pre-scaled, so glyph.font_size is already in physical pixels.
            let Some(bmp) = rasterize_colr_glyph(
                bytes,
                face_index,
                glyph.glyph_id as u16,
                glyph.font_size,
                glyph.color.to_rgba8(),
            ) else {
                continue;
            };

            // The shared rasterizer returns straight alpha; tiny-skia blits premultiplied.
            let mut pixels = bmp.rgba;
            renderer_core::premultiply_rgba(&mut pixels);
            let Some(size) = IntSize::from_wh(bmp.width, bmp.height) else {
                continue;
            };
            let Some(src) = Pixmap::from_vec(pixels, size) else {
                continue;
            };

            // placement_top rows of the bitmap sit above the baseline; blit so the baseline lands at glyph.y.
            let dst_x = glyph.x as i32 + bmp.placement_left;
            let dst_y = glyph.y as i32 - bmp.placement_top;
            pixmap.draw_pixmap(
                dst_x,
                dst_y,
                src.as_ref(),
                &PixmapPaint {
                    blend_mode: BlendMode::SourceOver,
                    ..Default::default()
                },
                outer_transform,
                outer_clip,
            );
        }
    }
}
