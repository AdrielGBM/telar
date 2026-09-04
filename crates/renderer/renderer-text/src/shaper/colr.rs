//! COLR glyphs: the colour-font path swash cannot rasterize, and the flag that skips looking for one.

use super::TextShaper;
use super::cache::{hash_text, text_style_bits};
use super::make_buffer;
use cosmic_text::{CacheKey, fontdb};
use geometry_core::Rect;
use renderer_core::{Color, TextStyle};
use std::sync::Arc;

/// A COLR color glyph swash could not rasterize (commonly emoji, but COLR v1 also covers colored icons and decorative glyphs). Collected by the software renderer for skrifa COLR fallback rendering.
pub struct ColrGlyph {
    pub font_id: fontdb::ID,
    pub glyph_id: u32,
    pub x: f32,
    pub y: f32,
    pub font_size: f32,
    pub color: Color,
}

impl TextShaper {
    /// Collects glyphs swash could not rasterize (COLR color glyphs, e.g. emoji) so the software renderer can re-rasterize them via the skrifa COLR fallback.
    pub fn collect_colr_glyphs(
        &mut self,
        text: &str,
        rect: Rect,
        style: &TextStyle,
        out: &mut Vec<ColrGlyph>,
    ) {
        if text.is_empty() {
            return;
        }
        // Plain UI text is the overwhelmingly common case, so once a (text, font_size) is known to shape to zero COLR glyphs the whole buffer build and per-glyph probe is skipped. COLR-ness depends only on the font and codepoint, never on rect or wrap, so the flag is layout-independent.
        let flag_key = (
            hash_text(text),
            style.font_size.to_bits(),
            text_style_bits(style),
        );
        if self.has_colr_cache.get(&flag_key) == Some(&false) {
            return;
        }
        let start_len = out.len();
        let buffer = make_buffer(&mut self.font_system, text, None, rect, style);
        let color = style.color.solid_color();
        for run in buffer.layout_runs() {
            for glyph in run.glyphs.iter() {
                let physical = glyph.physical((0., run.line_y), 1.0);
                let swash_empty = match self
                    .swash_cache
                    .get_image(&mut self.font_system, physical.cache_key)
                {
                    // swash returns `None` for COLR v1 glyphs it cannot rasterize, or `Some` with a zero-size placement for fonts that store empty outlines in `glyf` while the real rendering lives in COLR.
                    None => true,
                    Some(img) => img.placement.width == 0 && img.placement.height == 0,
                };
                if swash_empty {
                    out.push(ColrGlyph {
                        font_id: glyph.font_id,
                        glyph_id: glyph.glyph_id as u32,
                        x: rect.x + glyph.x,
                        y: rect.y + run.line_y,
                        font_size: style.font_size,
                        color,
                    });
                }
            }
        }
        // Recorded so later calls can short-circuit.
        self.has_colr_cache.insert(flag_key, out.len() > start_len);
    }

    /// Rasterizes a COLR v1 color glyph swash could not handle, returning atlas-ready data `(w, h, placement_left, placement_top, straight-alpha RGBA8, is_color_glyph=true)`.
    // `pub(super)` for `shaper::layout`, as the fallback when swash cannot rasterize a glyph.
    pub(super) fn rasterize_colr_atlas_glyph(
        &mut self,
        cache_key: CacheKey,
        physical_font_size: f32,
        foreground: [u8; 4],
    ) -> Option<(u32, u32, i32, i32, Vec<u8>, bool)> {
        let font = self.colr_font_bytes_impl(cache_key.font_id)?;
        let (bytes, face_index) = font.as_ref();
        let bmp = crate::colr::rasterize_colr_glyph(
            bytes,
            *face_index,
            cache_key.glyph_id,
            physical_font_size,
            foreground,
        )?;
        Some((
            bmp.width,
            bmp.height,
            bmp.placement_left,
            bmp.placement_top,
            bmp.rgba,
            true,
        ))
    }

    /// Cached raw font bytes + face index for `font_id`, for the software COLR fallback. Routes the software path through the same per-font cache the GPU atlas path uses, so emoji bytes (often several MB, e.g. NotoColorEmoji) are read once instead of re-read and copied on every frame.
    pub fn colr_font_bytes(&mut self, font_id: fontdb::ID) -> Option<Arc<(Vec<u8>, u32)>> {
        self.colr_font_bytes_impl(font_id)
    }

    /// Bench/test helper: id of the default sans-serif face, or `None` if the font system resolved none.
    #[doc(hidden)]
    pub fn default_face_id(&self) -> Option<fontdb::ID> {
        self.default_font_id()
    }

    /// Returns cached raw font bytes + face index for `font_id`, reading them from the font db on first use.
    fn colr_font_bytes_impl(&mut self, font_id: fontdb::ID) -> Option<Arc<(Vec<u8>, u32)>> {
        if let Some(b) = self.colr_font_cache.get(&font_id) {
            return Some(b.clone());
        }
        let (bytes, index) = self.font_data_for(font_id)?;
        let arc = Arc::new((bytes, index));
        self.colr_font_cache.insert(font_id, arc.clone());
        Some(arc)
    }
}
