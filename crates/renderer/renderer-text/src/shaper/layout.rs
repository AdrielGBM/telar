//! Shaping a string into positioned glyphs, and rasterizing the ones the atlas does not hold.

use super::TextShaper;
use super::cache::{ShapingCacheKey, hash_text, text_style_bits};
use super::{
    effective_line_height, from_cosmic_color, make_buffer, physical_glyph, resolve_coverage,
};
use cosmic_text::{CacheKey, SwashContent};
use geometry_core::{Color, Rect};
use renderer_core::{Raster, Span, TextStyle, TextWrap};

use super::atlas::GlyphInfo;

impl TextShaper {
    /// Lays `text` into `rect`, `spans` colouring and restyling the byte ranges that differ from `style`.
    ///
    /// A paragraph with spans skips the shaping cache, as the rich path always did: they are few and short, and each glyph carries its own colour, which the position-only cache entry has no room for. The glyph atlas still caches every raster either way.
    pub fn layout_glyphs(
        &mut self,
        text: &str,
        spans: Option<&[Span]>,
        rect: Rect,
        style: &TextStyle,
        scale_factor: f32,
        out: &mut Vec<GlyphInfo>,
    ) {
        out.clear();

        let font_size = style.font_size;
        let color = style.color.solid_color();
        let width = rect.width.ceil() as u32;
        let height = rect.height.ceil() as u32;

        if width == 0 || height == 0 || text.is_empty() {
            return;
        }

        if let Some(spans) = spans.filter(|s| !s.is_empty()) {
            let buffer = make_buffer(&mut self.font_system, text, Some(spans), rect, style);
            let mut glyphs: Vec<(CacheKey, i32, i32, Color)> = Vec::new();
            for run in buffer.layout_runs() {
                for glyph in run.glyphs.iter() {
                    let physical = physical_glyph(
                        glyph,
                        (0., run.line_y * scale_factor),
                        scale_factor,
                        style.raster,
                    );
                    let glyph_color = glyph.color_opt.map(from_cosmic_color).unwrap_or(color);
                    glyphs.push((physical.cache_key, physical.x, physical.y, glyph_color));
                }
            }
            drop(buffer);
            out.reserve(glyphs.len());
            for (cache_key, px, py, glyph_color) in glyphs {
                self.emit_glyph(
                    cache_key,
                    px,
                    py,
                    rect,
                    font_size,
                    scale_factor,
                    glyph_color,
                    style.raster,
                    out,
                );
            }
            return;
        }

        let shaping_key = ShapingCacheKey {
            text_hash: hash_text(text),
            font_size_bits: font_size.to_bits(),
            width,
            scale_factor_bits: scale_factor.to_bits(),
            style_bits: text_style_bits(style),
        };

        let positions: std::sync::Arc<Vec<(CacheKey, i32, i32)>> =
            if let Some(cached) = self.shaping_cache.get(&shaping_key) {
                cached.clone()
            } else {
                let buffer = make_buffer(&mut self.font_system, text, None, rect, style);
                let mut pos: Vec<(CacheKey, i32, i32)> = Vec::new();
                for run in buffer.layout_runs() {
                    for glyph in run.glyphs.iter() {
                        // cosmic-text's `physical` adds the offset without scaling it, and `screen_y` below divides the whole thing by the scale factor, so `line_y` must be pre-scaled here or every line packs onto the first at high DPI.
                        let physical = physical_glyph(
                            glyph,
                            (0., run.line_y * scale_factor),
                            scale_factor,
                            style.raster,
                        );
                        pos.push((physical.cache_key, physical.x, physical.y));
                    }
                }
                drop(buffer);
                let arc = std::sync::Arc::new(pos);
                self.shaping_cache.insert(shaping_key, arc.clone());
                arc
            };

        out.reserve(positions.len());
        for &(cache_key, px, py) in positions.iter() {
            self.emit_glyph(
                cache_key,
                px,
                py,
                rect,
                font_size,
                scale_factor,
                color,
                style.raster,
                out,
            );
        }
    }

    /// Fetches (rasterizing and atlas-caching on a miss) one shaped glyph and appends its quad to `out`, tinted `color`. The atlas stores mask glyphs white and the tint is applied here — the seam that lets a rich paragraph colour each run differently while sharing one glyph cache.
    #[allow(clippy::too_many_arguments)]
    fn emit_glyph(
        &mut self,
        cache_key: CacheKey,
        px: i32,
        py: i32,
        rect: Rect,
        font_size: f32,
        scale_factor: f32,
        color: Color,
        raster: Raster,
        out: &mut Vec<GlyphInfo>,
    ) {
        let tint = color.to_array();
        let identity_tint = [1.0, 1.0, 1.0, 1.0];
        // The placement offsets are whole physical pixels but the text box's own origin is not, and the atlas is sampled with a linear filter, so a quad landing half a texel off smears every glyph edge back into the blend the pixel raster exists to remove. Snapped in physical space, the grid the sampler resolves against.
        let (origin_x, origin_y) = match raster {
            Raster::Smooth => (rect.x * scale_factor, rect.y * scale_factor),
            Raster::Pixel => (
                (rect.x * scale_factor).round(),
                (rect.y * scale_factor).round(),
            ),
        };

        if let Some(entry) = self.atlas.fetch(&cache_key) {
            // Physical pixels, so divide by the scale factor for the logical coordinates the viewport shader expects.
            let screen_x = (origin_x + px as f32 + entry.placement_left as f32) / scale_factor;
            let screen_y = (origin_y + py as f32 - entry.placement_top as f32) / scale_factor;
            let glyph_color = if entry.is_color_glyph {
                identity_tint
            } else {
                tint
            };
            out.push(GlyphInfo {
                dest_rect: [
                    screen_x,
                    screen_y,
                    entry.glyph_width as f32 / scale_factor,
                    entry.glyph_height as f32 / scale_factor,
                ],
                uv_min: entry.uv_min,
                uv_max: entry.uv_max,
                color: glyph_color,
            });
            return;
        }

        if self.blank_glyphs.contains(&cache_key) {
            return;
        }

        // swash returns a usable bitmap for normal and colour glyphs; for COLR v1 it returns `None` or an empty placement, and skrifa rasterizes instead.
        let raster: Option<(u32, u32, i32, i32, Vec<u8>, bool)> = {
            let img_opt = self.swash_cache.get_image(&mut self.font_system, cache_key);
            match img_opt {
                Some(img) if img.placement.width != 0 && img.placement.height != 0 => {
                    let w = img.placement.width;
                    let h = img.placement.height;
                    let pl = img.placement.left;
                    let pt = img.placement.top;
                    let (pixels, is_color_glyph) = match img.content {
                        SwashContent::Mask => {
                            let mut out = vec![0u8; (w * h * 4) as usize];
                            for (i, &mask) in img.data.iter().enumerate() {
                                out[i * 4] = 255;
                                out[i * 4 + 1] = 255;
                                out[i * 4 + 2] = 255;
                                out[i * 4 + 3] = resolve_coverage(mask, raster);
                            }
                            (out, false)
                        }
                        SwashContent::SubpixelMask => {
                            let mut out = vec![0u8; (w * h * 4) as usize];
                            for (i, chunk) in img.data.chunks_exact(3).enumerate() {
                                // Known limitation: subpixel AA needs per-channel alpha compositing in the renderer to preserve the per-colour masks. Averaging RGB to greyscale loses that, which reads worse on LCD screens.
                                let mask = ((chunk[0] as u32 + chunk[1] as u32 + chunk[2] as u32)
                                    / 3) as u8;
                                out[i * 4] = 255;
                                out[i * 4 + 1] = 255;
                                out[i * 4 + 2] = 255;
                                out[i * 4 + 3] = resolve_coverage(mask, raster);
                            }
                            (out, false)
                        }
                        SwashContent::Color => (img.data.to_vec(), true),
                    };
                    Some((w, h, pl, pt, pixels, is_color_glyph))
                }
                _ => None,
            }
        };

        let (w, h, pl, pt, pixels, is_color_glyph) = match raster {
            Some(r) => r,
            None => match self.rasterize_colr_atlas_glyph(
                cache_key,
                font_size * scale_factor,
                color.to_rgba8(),
            ) {
                Some(r) => r,
                None => {
                    self.blank_glyphs.insert(cache_key);
                    return;
                }
            },
        };

        let entry = if let Some(e) =
            self.atlas
                .insert(cache_key, &pixels, w, h, pl, pt, is_color_glyph)
        {
            e
        } else {
            let mut inserted = None;
            loop {
                if let Some(evicted_key) = self.atlas.evict_lru() {
                    self.swash_cache.image_cache.remove(&evicted_key);
                    if let Some(e) =
                        self.atlas
                            .insert(cache_key, &pixels, w, h, pl, pt, is_color_glyph)
                    {
                        inserted = Some(e);
                        break;
                    }
                } else {
                    break;
                }
            }
            match inserted {
                Some(e) => e,
                None => return,
            }
        };

        let screen_x = (origin_x + px as f32 + pl as f32) / scale_factor;
        let screen_y = (origin_y + py as f32 - pt as f32) / scale_factor;
        let glyph_color = if is_color_glyph { identity_tint } else { tint };
        out.push(GlyphInfo {
            dest_rect: [
                screen_x,
                screen_y,
                w as f32 / scale_factor,
                h as f32 / scale_factor,
            ],
            uv_min: entry.uv_min,
            uv_max: entry.uv_max,
            color: glyph_color,
        });
    }

    pub fn measure_text(
        &mut self,
        text: &str,
        spans: Option<&[Span]>,
        max_width: f32,
        style: &TextStyle,
    ) -> (f32, f32) {
        if text.is_empty() {
            return (0.0, 0.0);
        }

        // A no-wrap style has no wrap width to be zero: it measures its natural width whatever box it was offered, which is the whole point of asking for one line.
        let width_u32 = max_width.ceil() as u32;
        if width_u32 == 0 && style.text_wrap == TextWrap::Wrap {
            return (0.0, 0.0);
        }

        // Uncached like `layout_glyphs`: the key is the paragraph's own style, which two paragraphs differing only in spans would share.
        let spans = spans.filter(|s| !s.is_empty());
        let cache_key = (
            hash_text(text),
            max_width.to_bits(),
            style.font_size.to_bits(),
            text_style_bits(style),
        );
        if spans.is_none()
            && let Some(&cached) = self.measure_cache.get(&cache_key)
        {
            return cached;
        }

        let rect = Rect {
            x: 0.0,
            y: 0.0,
            width: max_width,
            height: 100000.0,
        };
        // `make_buffer` already applies the clamp, so the measured extent reflects it.
        let buffer = make_buffer(&mut self.font_system, text, spans, rect, style);

        let mut width: f32 = 0.0;
        let mut height: f32 = 0.0;
        let line_height = effective_line_height(style);

        for run in buffer.layout_runs() {
            // From the line's top, not its baseline: `line_y` is where the letters sit, so adding a line height to it counted the ascent twice and every text box reserved nearly half again what it draws.
            height = run.line_top + line_height;
            // The line's advance width, not the glyph ink boxes: the last glyph's advance extends past its ink, so an ink-based width leaves the box a hair too narrow and the renderer wraps the final glyph.
            width = width.max(run.line_w);
        }

        // Rounded up, so sub-pixel rounding never re-wraps the last glyph.
        let result = (width.ceil(), height);
        if spans.is_none() {
            self.measure_cache.insert(cache_key, result);
        }
        result
    }

    /// The text's ink bounding box measured from the top of a `[0, max_width] × [0, ∞]` layout rect, at scale 1.0: `(ink_top, ink_height)`. Unlike [`measure_text`](Self::measure_text) (which returns the full line-box height) this is the actual drawn glyph extent, so a caller can *optically* center text — the font's line box reserves ascent room for accents/descenders that short runs like "72%" leave empty, which makes line-box-centered text sit visibly high next to an icon. Empty text or a run with no inked glyphs returns `(0.0, 0.0)`.
    pub fn measure_ink_bounds(
        &mut self,
        text: &str,
        max_width: f32,
        style: &TextStyle,
    ) -> (f32, f32) {
        if text.is_empty() || max_width.ceil() as u32 == 0 {
            return (0.0, 0.0);
        }
        let rect = Rect {
            x: 0.0,
            y: 0.0,
            width: max_width,
            height: 100000.0,
        };
        let buffer = make_buffer(&mut self.font_system, text, None, rect, style);
        // Collected first, so the immutable `layout_runs` borrow ends before the mutable lookups below.
        let mut glyphs: Vec<(CacheKey, i32)> = Vec::new();
        for run in buffer.layout_runs() {
            for glyph in run.glyphs.iter() {
                let p = physical_glyph(glyph, (0.0, run.line_y), 1.0, style.raster);
                glyphs.push((p.cache_key, p.y));
            }
        }
        drop(buffer);

        let mut min_top = f32::INFINITY;
        let mut max_bottom = f32::NEG_INFINITY;
        for (cache_key, baseline_y) in glyphs {
            if let Some(img) = self.swash_cache.get_image(&mut self.font_system, cache_key) {
                if img.placement.width == 0 || img.placement.height == 0 {
                    continue;
                }
                let top = baseline_y as f32 - img.placement.top as f32;
                min_top = min_top.min(top);
                max_bottom = max_bottom.max(top + img.placement.height as f32);
            }
        }
        if !min_top.is_finite() || max_bottom <= min_top {
            return (0.0, 0.0);
        }
        (min_top, max_bottom - min_top)
    }
}
