use super::TextShaper;
use super::cache::{AlphaCacheKey, hash_text, make_text_cache_key, text_style_bits};
use super::{make_buffer, make_buffer_rich};
use cosmic_text::Color as CosmicColor;
use geometry_core::Rect;
use renderer_core::{TextRun, TextStyle, premultiply_rgba};
use std::sync::Arc;

impl TextShaper {
    pub fn rasterize(
        &mut self,
        text: &str,
        rect: Rect,
        style: &TextStyle,
    ) -> (Arc<[u8]>, u32, u32) {
        let font_size = style.font_size;
        let color = style.paint.solid_color();
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

        if let Some(cached) = self.pixel_cache.get(&key) {
            return (Arc::clone(cached), width, height);
        }

        let rgba = color.to_rgba8();
        let [r, g, b, a] = rgba;
        let cosmic_color = CosmicColor::rgba(r, g, b, a);

        let mut buffer = make_buffer(&mut self.font_system, text, rect, style);

        let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
        buffer.draw(
            &mut self.font_system,
            &mut self.swash_cache,
            cosmic_color,
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
        let _ = self.pixel_cache.put_with_weight(key, arc.clone());

        (arc, width, height)
    }

    /// Rasterizes a rich paragraph (styled runs) to a premultiplied RGBA block. `Buffer::draw` honours each
    /// glyph's own `color_opt` (set per run by `make_buffer_rich`), so runs paint in their own colours; the
    /// `default` colour only covers glyphs without one. Uncached — rich blocks are dynamic and few.
    pub fn rasterize_rich(
        &mut self,
        runs: &[TextRun],
        rect: Rect,
        base: &TextStyle,
    ) -> (Arc<[u8]>, u32, u32) {
        let width = rect.width.ceil() as u32;
        let height = rect.height.ceil() as u32;
        if width == 0 || height == 0 {
            return (Arc::from([].as_slice()), 0, 0);
        }

        let [r, g, b, a] = base.paint.solid_color().to_rgba8();
        let default = CosmicColor::rgba(r, g, b, a);
        let mut buffer = make_buffer_rich(&mut self.font_system, runs, rect, base);

        let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
        buffer.draw(
            &mut self.font_system,
            &mut self.swash_cache,
            default,
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

        premultiply_rgba(&mut pixels);
        (Arc::from(pixels.into_boxed_slice()), width, height)
    }

    pub fn rasterize_alpha(
        &mut self,
        text: &str,
        rect: Rect,
        style: &TextStyle,
    ) -> (Arc<[u8]>, u32, u32) {
        let font_size = style.font_size;
        let width = rect.width.ceil() as u32;
        let height = rect.height.ceil() as u32;

        if width == 0 || height == 0 {
            return (Arc::from([].as_slice()), 0, 0);
        }

        let key = AlphaCacheKey {
            text_hash: hash_text(text),
            font_size_bits: font_size.to_bits(),
            width,
            height,
            style_bits: text_style_bits(style),
        };

        if let Some(cached) = self.alpha_pixel_cache.get(&key) {
            return (Arc::clone(cached), width, height);
        }

        let white = CosmicColor::rgba(255, 255, 255, 255);

        let mut buffer = make_buffer(&mut self.font_system, text, rect, style);

        let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
        buffer.draw(
            &mut self.font_system,
            &mut self.swash_cache,
            white,
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

        premultiply_rgba(&mut pixels);
        let arc: Arc<[u8]> = Arc::from(pixels.into_boxed_slice());
        let _ = self.alpha_pixel_cache.put_with_weight(key, arc.clone());

        (arc, width, height)
    }
}
