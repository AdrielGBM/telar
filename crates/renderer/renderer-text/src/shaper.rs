use clru::{CLruCache, CLruCacheConfig, WeightScale};
use cosmic_text::{
    Attrs, Buffer, CacheKey, Color as CosmicColor, FontSystem, Metrics, Shaping, SwashCache,
    SwashContent,
};
use etagere::{AllocId, AtlasAllocator, size2};
use renderer_core::{Color, Rect, TextStyle, premultiply_rgba};
use std::collections::{HashMap, VecDeque};
use std::num::NonZeroUsize;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ShapingCacheKey {
    pub text: String,
    pub font_size_bits: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TextCacheKey {
    pub text: String,
    pub font_size_bits: u32,
    pub width: u32,
    pub height: u32,
    pub color_packed: u32,
}

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    pub cache_key: CacheKey,
}

#[derive(Clone, Copy, Debug)]
pub struct AtlasEntry {
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
    pub glyph_width: u32,
    pub glyph_height: u32,
    pub placement_left: i32,
    pub placement_top: i32,
    pub is_color_glyph: bool,
    alloc_id: AllocId,
    lru_gen: u64,
}

pub struct GlyphInfo {
    pub dest_rect: [f32; 4],
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
    pub color: [f32; 4],
}

pub const ATLAS_SIZE: u32 = 2048;

pub struct GlyphAtlas {
    pub pixels: Vec<u8>,
    pub dirty: bool,
    entries: HashMap<GlyphKey, AtlasEntry>,
    allocator: AtlasAllocator,
    lru_queue: VecDeque<(GlyphKey, u64)>,
    lru_counter: u64,
}

impl GlyphAtlas {
    pub fn new() -> Self {
        Self {
            pixels: vec![0u8; (ATLAS_SIZE * ATLAS_SIZE * 4) as usize],
            dirty: true,
            entries: HashMap::new(),
            allocator: AtlasAllocator::new(size2(ATLAS_SIZE as i32, ATLAS_SIZE as i32)),
            lru_queue: VecDeque::new(),
            lru_counter: 0,
        }
    }

    pub fn clear(&mut self) {
        self.pixels.fill(0);
        self.entries.clear();
        self.allocator.clear();
        self.lru_queue.clear();
        self.lru_counter = 0;
        self.dirty = true;
    }

    fn insert(
        &mut self,
        key: GlyphKey,
        pixels: &[u8],
        w: u32,
        h: u32,
        placement_left: i32,
        placement_top: i32,
        is_color_glyph: bool,
    ) -> Option<AtlasEntry> {
        let alloc = self.allocator.allocate(size2(w as i32, h as i32))?;
        let rect = alloc.rectangle;
        let alloc_id = alloc.id;
        let ax = rect.min.x as u32;
        let ay = rect.min.y as u32;

        for row in 0..h as usize {
            let src = &pixels[row * w as usize * 4..(row + 1) * w as usize * 4];
            let dst_start = ((ay as usize + row) * ATLAS_SIZE as usize + ax as usize) * 4;
            self.pixels[dst_start..dst_start + w as usize * 4].copy_from_slice(src);
        }

        let stamp = self.lru_counter;
        self.lru_counter += 1;
        let entry = AtlasEntry {
            uv_min: [ax as f32 / ATLAS_SIZE as f32, ay as f32 / ATLAS_SIZE as f32],
            uv_max: [
                (ax + w) as f32 / ATLAS_SIZE as f32,
                (ay + h) as f32 / ATLAS_SIZE as f32,
            ],
            glyph_width: w,
            glyph_height: h,
            placement_left,
            placement_top,
            is_color_glyph,
            alloc_id,
            lru_gen: stamp,
        };
        self.entries.insert(key, entry);
        self.lru_queue.push_back((key, stamp));
        self.dirty = true;
        Some(entry)
    }

    pub fn get_and_touch(&mut self, key: &GlyphKey) -> Option<AtlasEntry> {
        let entry = self.entries.get_mut(key)?;
        let stamp = self.lru_counter;
        self.lru_counter += 1;
        entry.lru_gen = stamp;
        self.lru_queue.push_back((*key, stamp));
        Some(*entry)
    }

    /// Evicts one LRU entry from the glyph atlas when it becomes full. Uses capacity-based LRU (evict when full) rather than a frame threshold, because atlas space is the binding constraint.
    fn evict_lru(&mut self) -> bool {
        while let Some((key, stamp)) = self.lru_queue.pop_front() {
            if let Some(entry) = self.entries.get(&key) {
                if entry.lru_gen == stamp {
                    self.allocator.deallocate(entry.alloc_id);
                    self.entries.remove(&key);
                    self.dirty = true;
                    return true;
                }
            }
        }
        false
    }
}

impl Default for GlyphAtlas {
    fn default() -> Self {
        Self::new()
    }
}

struct PixelCacheScale;
impl WeightScale<TextCacheKey, Vec<u8>> for PixelCacheScale {
    fn weight(&self, _key: &TextCacheKey, value: &Vec<u8>) -> usize {
        value.len().max(1)
    }
}

const PIXEL_CACHE_BUDGET_BYTES: usize = 64 * 1024 * 1024;

pub struct TextShaper {
    pub font_system: FontSystem,
    pub swash_cache: SwashCache,
    pub atlas: GlyphAtlas,
    pixel_cache:
        CLruCache<TextCacheKey, Vec<u8>, std::collections::hash_map::RandomState, PixelCacheScale>,
    shaping_cache: CLruCache<ShapingCacheKey, Vec<(CacheKey, i32, i32)>>,
}

fn make_buffer(font_system: &mut FontSystem, text: &str, rect: Rect, font_size: f32) -> Buffer {
    let metrics = Metrics::new(font_size, font_size * 1.2);
    let mut buffer = Buffer::new(font_system, metrics);
    buffer.set_size(Some(rect.w), Some(rect.h));
    buffer.set_text(text, &Attrs::new(), Shaping::Advanced, None);
    buffer.shape_until_scroll(font_system, false);
    buffer
}

impl TextShaper {
    pub fn new() -> Self {
        Self {
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
            atlas: GlyphAtlas::new(),
            pixel_cache: CLruCache::with_config(
                CLruCacheConfig::new(NonZeroUsize::new(PIXEL_CACHE_BUDGET_BYTES).unwrap())
                    .with_scale(PixelCacheScale),
            ),
            shaping_cache: CLruCache::new(NonZeroUsize::new(2048).unwrap()),
        }
    }

    pub fn layout_glyphs(&mut self, text: &str, rect: Rect, style: &TextStyle) -> Vec<GlyphInfo> {
        let font_size = style.font_size;
        let color = style.color;
        let width = rect.w.ceil() as u32;
        let height = rect.h.ceil() as u32;

        if width == 0 || height == 0 || text.is_empty() {
            return Vec::new();
        }

        let tint = color.to_array();
        let identity_tint = [1.0, 1.0, 1.0, 1.0];

        let shaping_key = ShapingCacheKey {
            text: text.to_owned(),
            font_size_bits: font_size.to_bits(),
            width,
            height,
        };

        let positions = if let Some(cached) = self.shaping_cache.get(&shaping_key) {
            cached.clone()
        } else {
            let buffer = make_buffer(&mut self.font_system, text, rect, font_size);
            let mut pos: Vec<(CacheKey, i32, i32)> = Vec::new();
            for run in buffer.layout_runs() {
                for glyph in run.glyphs.iter() {
                    let physical = glyph.physical((0., run.line_y), 1.0);
                    pos.push((physical.cache_key, physical.x, physical.y));
                }
            }
            drop(buffer);
            let _ = self.shaping_cache.put(shaping_key, pos.clone());
            pos
        };

        let mut result = Vec::with_capacity(positions.len());

        for (cache_key, px, py) in positions {
            let glyph_key = GlyphKey { cache_key };

            if let Some(entry) = self.atlas.get_and_touch(&glyph_key) {
                let screen_x = rect.x + px as f32 + entry.placement_left as f32;
                let screen_y = rect.y + py as f32 - entry.placement_top as f32;
                let glyph_color = if entry.is_color_glyph {
                    identity_tint
                } else {
                    tint
                };
                result.push(GlyphInfo {
                    dest_rect: [
                        screen_x,
                        screen_y,
                        entry.glyph_width as f32,
                        entry.glyph_height as f32,
                    ],
                    uv_min: entry.uv_min,
                    uv_max: entry.uv_max,
                    color: glyph_color,
                });
                continue;
            }

            let raster = {
                let img_opt = self.swash_cache.get_image(&mut self.font_system, cache_key);
                match img_opt {
                    None => continue,
                    Some(img) => {
                        let w = img.placement.width;
                        let h = img.placement.height;
                        if w == 0 || h == 0 {
                            continue;
                        }
                        let pl = img.placement.left;
                        let pt = img.placement.top;
                        let (pixels, is_color_glyph) = match img.content {
                            SwashContent::Mask => {
                                let mut out = vec![0u8; (w * h * 4) as usize];
                                for (i, &mask) in img.data.iter().enumerate() {
                                    out[i * 4] = 255;
                                    out[i * 4 + 1] = 255;
                                    out[i * 4 + 2] = 255;
                                    out[i * 4 + 3] = mask;
                                }
                                (out, false)
                            }
                            SwashContent::SubpixelMask => {
                                let mut out = vec![0u8; (w * h * 4) as usize];
                                for (i, chunk) in img.data.chunks_exact(3).enumerate() {
                                    // KNOWN LIMITATION: Subpixel anti-aliasing (LCD rendering) requires per-channel alpha compositing in the renderer to preserve per-color subpixel masks. Currently, we average the RGB channels to grayscale, losing the color-specific AA information. This produces visually inferior text on LCD screens. Supporting proper subpixel AA would require renderer-level per-channel compositing, which is not yet implemented.
                                    let mask =
                                        ((chunk[0] as u32 + chunk[1] as u32 + chunk[2] as u32) / 3)
                                            as u8;
                                    out[i * 4] = 255;
                                    out[i * 4 + 1] = 255;
                                    out[i * 4 + 2] = 255;
                                    out[i * 4 + 3] = mask;
                                }
                                (out, false)
                            }
                            SwashContent::Color => (img.data.to_vec(), true),
                        };
                        (w, h, pl, pt, pixels, is_color_glyph)
                    }
                }
            };

            let (w, h, pl, pt, pixels, is_color_glyph) = raster;

            let entry = if let Some(e) =
                self.atlas
                    .insert(glyph_key, &pixels, w, h, pl, pt, is_color_glyph)
            {
                e
            } else {
                let mut inserted = None;
                loop {
                    if !self.atlas.evict_lru() {
                        break;
                    }
                    if let Some(e) =
                        self.atlas
                            .insert(glyph_key, &pixels, w, h, pl, pt, is_color_glyph)
                    {
                        inserted = Some(e);
                        break;
                    }
                }
                match inserted {
                    Some(e) => e,
                    None => continue,
                }
            };

            let screen_x = rect.x + px as f32 + pl as f32;
            let screen_y = rect.y + py as f32 - pt as f32;
            let glyph_color = if is_color_glyph { identity_tint } else { tint };
            result.push(GlyphInfo {
                dest_rect: [screen_x, screen_y, w as f32, h as f32],
                uv_min: entry.uv_min,
                uv_max: entry.uv_max,
                color: glyph_color,
            });
        }

        result
    }

    pub fn rasterize(
        &mut self,
        text: &str,
        rect: Rect,
        style: &TextStyle,
    ) -> (TextCacheKey, Vec<u8>, u32, u32) {
        let font_size = style.font_size;
        let color = style.color;
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

        let mut buffer = make_buffer(&mut self.font_system, text, rect, font_size);

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
                            pixels[idx] = color.r();
                            pixels[idx + 1] = color.g();
                            pixels[idx + 2] = color.b();
                            pixels[idx + 3] = color.a();
                        }
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
        self.pixel_cache
            .put_with_weight(key.clone(), pixels.clone())
            .ok();

        (key, pixels, width, height)
    }
}

impl Default for TextShaper {
    fn default() -> Self {
        Self::new()
    }
}
