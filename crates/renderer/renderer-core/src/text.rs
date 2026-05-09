use crate::{Color, Rect};
use cosmic_text::{Attrs, Buffer, Color as CosmicColor, FontSystem, Metrics, Shaping, SwashCache};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TextCacheKey {
    pub text: String,
    pub font_size_bits: u32,
    pub width: u32,
    pub height: u32,
    pub color_packed: u32,
}

const PIXEL_CACHE_MAX: usize = 512;

pub fn make_text_cache_key(
    text: &str,
    font_size: f32,
    width: u32,
    height: u32,
    color: Color,
) -> TextCacheKey {
    let rgba = color.to_rgba8();
    let color_packed = u32::from_le_bytes(rgba);
    TextCacheKey {
        text: text.to_owned(),
        font_size_bits: font_size.to_bits(),
        width,
        height,
        color_packed,
    }
}

pub struct TextShaper {
    pub font_system: FontSystem,
    pub swash_cache: SwashCache,
    pixel_cache: HashMap<TextCacheKey, Vec<u8>>,
}

impl TextShaper {
    pub fn new() -> Self {
        Self {
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
            pixel_cache: HashMap::new(),
        }
    }

    pub fn rasterize(
        &mut self,
        text: &str,
        rect: Rect,
        font_size: f32,
        color: Color,
    ) -> (TextCacheKey, Vec<u8>, u32, u32) {
        let width = rect.w.ceil() as u32;
        let height = rect.h.ceil() as u32;

        let key = make_text_cache_key(text, font_size, width, height, color);

        if width == 0 || height == 0 {
            return (key, Vec::new(), 0, 0);
        }

        if let Some(cached) = self.pixel_cache.get(&key) {
            return (key, cached.clone(), width, height);
        }

        let rgba = color.to_rgba8();
        let [r, g, b, a] = rgba;
        let cosmic_color = CosmicColor::rgba(r, g, b, a);

        let metrics = Metrics::new(font_size, font_size * 1.2);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_size(Some(rect.w), Some(rect.h));
        buffer.set_text(text, &Attrs::new(), Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.font_system, false);

        let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
        buffer.draw(
            &mut self.font_system,
            &mut self.swash_cache,
            cosmic_color,
            |bx, by, bw, bh, color| {
                for row in 0..bh as usize {
                    for col in 0..bw as usize {
                        let px = bx + col as i32;
                        let py = by + row as i32;
                        if px >= 0
                            && py >= 0
                            && (px as usize) < width as usize
                            && (py as usize) < height as usize
                        {
                            let idx = (py as usize * width as usize + px as usize) * 4;
                            let a = color.a() as u32;
                            pixels[idx] = (color.r() as u32 * a / 255) as u8;
                            pixels[idx + 1] = (color.g() as u32 * a / 255) as u8;
                            pixels[idx + 2] = (color.b() as u32 * a / 255) as u8;
                            pixels[idx + 3] = a as u8;
                        }
                    }
                }
            },
        );

        if self.pixel_cache.len() >= PIXEL_CACHE_MAX {
            if let Some(old_key) = self.pixel_cache.keys().next().cloned() {
                self.pixel_cache.remove(&old_key);
            }
        }
        self.pixel_cache.insert(key.clone(), pixels.clone());

        (key, pixels, width, height)
    }
}

impl Default for TextShaper {
    fn default() -> Self {
        Self::new()
    }
}
