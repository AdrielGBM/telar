use super::TextShaper;
use super::cache::{make_text_cache_key, text_style_bits};
use super::{make_buffer, physical_glyph, resolve_coverage};
use cosmic_text::{Buffer, Color as CosmicColor};
use geometry_core::Rect;
use renderer_core::{Raster, Span, TextStyle, premultiply_rgba};
use std::sync::Arc;

impl TextShaper {
    /// Walks a shaped buffer's glyphs, handing each covered pixel to `callback` as
    /// `(x, y, w, h, color)` — the same shape [`Buffer::draw`] uses, and delegating to it outright for
    /// the smooth raster so that path stays byte-identical.
    ///
    /// The pixel raster needs its own walk because the two things it changes both live inside that
    /// call: where a glyph's origin is rounded, and what its coverage resolves to. Text decorations are
    /// not reproduced here — no `TextStyle` axis sets any, so a shaped buffer never carries one.
    fn draw_buffer(
        &mut self,
        buffer: &mut Buffer,
        default: CosmicColor,
        raster: Raster,
        mut callback: impl FnMut(i32, i32, u32, u32, CosmicColor),
    ) {
        if raster == Raster::Smooth {
            buffer.draw(
                &mut self.font_system,
                &mut self.swash_cache,
                default,
                callback,
            );
            return;
        }
        buffer.shape_until_scroll(&mut self.font_system, false);
        let placed: Vec<(cosmic_text::PhysicalGlyph, CosmicColor)> = buffer
            .layout_runs()
            .flat_map(|run| {
                run.glyphs.iter().map(move |glyph| {
                    (
                        physical_glyph(glyph, (0., run.line_y), 1.0, raster),
                        glyph.color_opt.unwrap_or(default),
                    )
                })
            })
            .collect();
        for (physical, color) in placed {
            self.swash_cache.with_pixels(
                &mut self.font_system,
                physical.cache_key,
                color,
                |x, y, pixel| {
                    let covered = CosmicColor::rgba(
                        pixel.r(),
                        pixel.g(),
                        pixel.b(),
                        resolve_coverage(pixel.a(), raster),
                    );
                    callback(physical.x + x, physical.y + y, 1, 1, covered);
                },
            );
        }
    }

    /// Rasterizes a paragraph to a premultiplied RGBA block. `Buffer::draw` honours each glyph's own
    /// `color_opt`, so a span paints in its own colour; `style`'s only covers glyphs without one.
    ///
    /// Spanned paragraphs are uncached, for the same reason `measure_text` does not cache them: the key is the
    /// paragraph's own style, which two paragraphs differing only in spans would share.
    pub fn rasterize(
        &mut self,
        text: &str,
        spans: Option<&[Span]>,
        rect: Rect,
        style: &TextStyle,
    ) -> (Arc<[u8]>, u32, u32) {
        let spans = spans.filter(|s| !s.is_empty());
        let font_size = style.font_size;
        let color = style.color.solid_color();
        let width = rect.width.ceil() as u32;
        let height = rect.height.ceil() as u32;

        let key = make_text_cache_key(
            text,
            font_size,
            width,
            height,
            color,
            text_style_bits(style),
        );

        if width == 0 || height == 0 {
            return (Arc::from([].as_slice()), 0, 0);
        }

        if spans.is_none()
            && let Some(cached) = self.raster_cache.get(&key)
        {
            return (Arc::clone(cached), width, height);
        }

        let rgba = color.to_rgba8();
        let [r, g, b, a] = rgba;
        let cosmic_color = CosmicColor::rgba(r, g, b, a);

        let mut buffer = make_buffer(&mut self.font_system, text, spans, rect, style);

        let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
        self.draw_buffer(
            &mut buffer,
            cosmic_color,
            style.raster,
            |bx, by, bw, bh, color| {
                let col_start = ((-bx).max(0)) as usize;
                let col_end = ((width as i32 - bx).max(0) as usize).min(bw as usize);
                let row_start = ((-by).max(0)) as usize;
                let row_end = ((height as i32 - by).max(0) as usize).min(bh as usize);
                for row in row_start..row_end {
                    for col in col_start..col_end {
                        let px = (bx + col as i32) as usize;
                        let py = (by + row as i32) as usize;
                        let idx = (py * width as usize + px) * 4;
                        pixels[idx] = color.r();
                        pixels[idx + 1] = color.g();
                        pixels[idx + 2] = color.b();
                        pixels[idx + 3] = color.a();
                    }
                }
            },
        );

        if a < 255 {
            for chunk in pixels.chunks_exact_mut(4) {
                chunk[3] = ((chunk[3] as u32 * a as u32) / 255) as u8;
            }
        }

        premultiply_rgba(&mut pixels);
        let arc: Arc<[u8]> = Arc::from(pixels.into_boxed_slice());
        if spans.is_none() {
            self.raster_cache.insert(key, arc.clone());
        }

        (arc, width, height)
    }

    /// Rasterizes `text` white-on-transparent, for a caller that will tint and blur it into a shadow.
    ///
    /// Uncached, unlike [`rasterize`](Self::rasterize). The one caller keeps the *blurred* result, which is what a
    /// later frame actually draws, so a cache here would hold the input to a computation whose output is already
    /// kept — 64 MB of budget for an intermediate. That only worked out to a saving while the caller rasterized on
    /// every frame before consulting its shadow cache; now it asks first and this runs on a miss.
    pub fn rasterize_alpha(
        &mut self,
        text: &str,
        rect: Rect,
        style: &TextStyle,
    ) -> (Arc<[u8]>, u32, u32) {
        let width = rect.width.ceil() as u32;
        let height = rect.height.ceil() as u32;

        if width == 0 || height == 0 {
            return (Arc::from([].as_slice()), 0, 0);
        }

        let white = CosmicColor::rgba(255, 255, 255, 255);

        let mut buffer = make_buffer(&mut self.font_system, text, None, rect, style);

        let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
        self.draw_buffer(&mut buffer, white, style.raster, |bx, by, bw, bh, color| {
            let col_start = ((-bx).max(0)) as usize;
            let col_end = ((width as i32 - bx).max(0) as usize).min(bw as usize);
            let row_start = ((-by).max(0)) as usize;
            let row_end = ((height as i32 - by).max(0) as usize).min(bh as usize);
            for row in row_start..row_end {
                for col in col_start..col_end {
                    let px = (bx + col as i32) as usize;
                    let py = (by + row as i32) as usize;
                    let idx = (py * width as usize + px) * 4;
                    pixels[idx] = color.r();
                    pixels[idx + 1] = color.g();
                    pixels[idx + 2] = color.b();
                    pixels[idx + 3] = color.a();
                }
            }
        });

        premultiply_rgba(&mut pixels);
        (Arc::from(pixels.into_boxed_slice()), width, height)
    }
}
