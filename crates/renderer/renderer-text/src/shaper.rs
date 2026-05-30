use clru::{CLruCache, CLruCacheConfig, WeightScale};
use cosmic_text::{
    Attrs, Buffer, CacheKey, Color as CosmicColor, FontSystem, Metrics, Shaping, SwashCache,
    SwashContent,
};
use etagere::{AllocId, AtlasAllocator, size2};
use geometry_core::Rect;
use renderer_core::{Color, TextStyle, premultiply_rgba};
use rustc_hash::{FxBuildHasher, FxHashMap, FxHasher};
use std::collections::VecDeque;
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ShapingCacheKey {
    pub text_hash: u64,
    pub font_size_bits: u32,
    pub width: u32,
    // height removed — shaping only depends on wrap width, not container height
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TextCacheKey {
    pub text_hash: u64,
    pub font_size_bits: u32,
    pub width: u32,
    pub height: u32,
    pub color_packed: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct AlphaCacheKey {
    text_hash: u64,
    font_size_bits: u32,
    width: u32,
    height: u32,
}

fn hash_text(text: &str) -> u64 {
    let mut h = FxHasher::default();
    text.hash(&mut h);
    h.finish()
}

#[inline]
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
        text_hash: hash_text(text),
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
    pub dirty_rects: Vec<[u32; 4]>,
    entries: FxHashMap<GlyphKey, AtlasEntry>,
    allocator: AtlasAllocator,
    lru_queue: VecDeque<(GlyphKey, u64)>,
    lru_counter: u64,
}

impl GlyphAtlas {
    pub fn new() -> Self {
        Self {
            pixels: vec![0u8; (ATLAS_SIZE * ATLAS_SIZE * 4) as usize],
            dirty_rects: Vec::new(),
            entries: FxHashMap::default(),
            allocator: AtlasAllocator::new(size2(ATLAS_SIZE as i32, ATLAS_SIZE as i32)),
            lru_queue: VecDeque::new(),
            lru_counter: 0,
        }
    }

    pub fn drain_dirty_rects(&mut self) -> std::vec::Drain<'_, [u32; 4]> {
        self.dirty_rects.drain(..)
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
        self.dirty_rects.push([ax, ay, w, h]);
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
impl WeightScale<TextCacheKey, Arc<[u8]>> for PixelCacheScale {
    fn weight(&self, _key: &TextCacheKey, value: &Arc<[u8]>) -> usize {
        value.len().max(1)
    }
}

struct AlphaCacheScale;
impl WeightScale<AlphaCacheKey, Arc<[u8]>> for AlphaCacheScale {
    fn weight(&self, _key: &AlphaCacheKey, value: &Arc<[u8]>) -> usize {
        value.len().max(1)
    }
}

struct ShapingCacheScale;
impl WeightScale<ShapingCacheKey, Arc<Vec<(CacheKey, i32, i32)>>> for ShapingCacheScale {
    fn weight(&self, _key: &ShapingCacheKey, value: &Arc<Vec<(CacheKey, i32, i32)>>) -> usize {
        value.len().saturating_mul(24).max(1)
    }
}

pub struct TextShaperConfig {
    pub pixel_cache_budget_bytes: usize,
    pub alpha_cache_budget_bytes: usize,
    pub shaping_cache_budget_bytes: usize,
}

impl Default for TextShaperConfig {
    fn default() -> Self {
        Self {
            pixel_cache_budget_bytes: 64 * 1024 * 1024,
            alpha_cache_budget_bytes: 64 * 1024 * 1024,
            shaping_cache_budget_bytes: 24 * 1024 * 1024,
        }
    }
}

pub struct TextShaper {
    font_system: FontSystem,
    swash_cache: SwashCache,
    pub atlas: GlyphAtlas,
    pixel_cache: CLruCache<TextCacheKey, Arc<[u8]>, FxBuildHasher, PixelCacheScale>,
    alpha_pixel_cache: CLruCache<AlphaCacheKey, Arc<[u8]>, FxBuildHasher, AlphaCacheScale>,
    shaping_cache: CLruCache<
        ShapingCacheKey,
        Arc<Vec<(CacheKey, i32, i32)>>,
        FxBuildHasher,
        ShapingCacheScale,
    >,
}

fn make_buffer(font_system: &mut FontSystem, text: &str, rect: Rect, font_size: f32) -> Buffer {
    let metrics = Metrics::new(font_size, font_size * 1.2);
    let mut buffer = Buffer::new(font_system, metrics);
    buffer.set_size(Some(rect.width), Some(rect.height));
    buffer.set_text(text, &Attrs::new(), Shaping::Advanced, None);
    buffer.shape_until_scroll(font_system, false);
    buffer
}

impl TextShaper {
    pub fn new() -> Self {
        Self::with_config(TextShaperConfig::default())
    }

    pub fn with_config(config: TextShaperConfig) -> Self {
        Self {
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
            atlas: GlyphAtlas::new(),
            pixel_cache: CLruCache::with_config(
                CLruCacheConfig::new(NonZeroUsize::new(config.pixel_cache_budget_bytes).unwrap())
                    .with_hasher(FxBuildHasher::default())
                    .with_scale(PixelCacheScale),
            ),
            alpha_pixel_cache: CLruCache::with_config(
                CLruCacheConfig::new(NonZeroUsize::new(config.alpha_cache_budget_bytes).unwrap())
                    .with_hasher(FxBuildHasher::default())
                    .with_scale(AlphaCacheScale),
            ),
            shaping_cache: CLruCache::with_config(
                CLruCacheConfig::new(NonZeroUsize::new(config.shaping_cache_budget_bytes).unwrap())
                    .with_hasher(FxBuildHasher::default())
                    .with_scale(ShapingCacheScale),
            ),
        }
    }

    pub fn layout_glyphs(
        &mut self,
        text: &str,
        rect: Rect,
        style: &TextStyle,
        out: &mut Vec<GlyphInfo>,
    ) {
        out.clear();

        let font_size = style.font_size;
        let color = style.paint.solid_color();
        let width = rect.width.ceil() as u32;
        let height = rect.height.ceil() as u32;

        if width == 0 || height == 0 || text.is_empty() {
            return;
        }

        let tint = color.to_array();
        let identity_tint = [1.0, 1.0, 1.0, 1.0];

        let shaping_key = ShapingCacheKey {
            text_hash: hash_text(text),
            font_size_bits: font_size.to_bits(),
            width,
        };

        let positions: std::sync::Arc<Vec<(CacheKey, i32, i32)>> =
            if let Some(cached) = self.shaping_cache.get(&shaping_key) {
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
                let arc = std::sync::Arc::new(pos);
                let _ = self.shaping_cache.put_with_weight(shaping_key, arc.clone());
                arc
            };

        out.reserve(positions.len());

        for &(cache_key, px, py) in positions.iter() {
            let glyph_key = GlyphKey { cache_key };

            if let Some(entry) = self.atlas.get_and_touch(&glyph_key) {
                let screen_x = rect.x + px as f32 + entry.placement_left as f32;
                let screen_y = rect.y + py as f32 - entry.placement_top as f32;
                let glyph_color = if entry.is_color_glyph {
                    identity_tint
                } else {
                    tint
                };
                out.push(GlyphInfo {
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
            out.push(GlyphInfo {
                dest_rect: [screen_x, screen_y, w as f32, h as f32],
                uv_min: entry.uv_min,
                uv_max: entry.uv_max,
                color: glyph_color,
            });
        }
    }

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

        let key = make_text_cache_key(text, font_size, width, height, color);

        if width == 0 || height == 0 {
            return (Arc::from([].as_slice()), 0, 0);
        }

        if let Some(cached) = self.pixel_cache.get(&key) {
            return (Arc::clone(cached), width, height);
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
        };

        if let Some(cached) = self.alpha_pixel_cache.get(&key) {
            return (Arc::clone(cached), width, height);
        }

        let white = CosmicColor::rgba(255, 255, 255, 255);

        let mut buffer = make_buffer(&mut self.font_system, text, rect, font_size);

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

    pub fn measure_text(&mut self, text: &str, max_width: f32, font_size: f32) -> (f32, f32) {
        if text.is_empty() {
            return (0.0, 0.0);
        }

        let width_u32 = max_width.ceil() as u32;
        if width_u32 == 0 {
            return (0.0, 0.0);
        }

        let rect = Rect {
            x: 0.0,
            y: 0.0,
            width: max_width,
            height: 100000.0,
        };
        let buffer = make_buffer(&mut self.font_system, text, rect, font_size);

        let mut width: f32 = 0.0;
        let mut height: f32 = 0.0;
        let line_height = font_size * 1.2;

        for run in buffer.layout_runs() {
            height = (run.line_y + line_height) as f32;
            for glyph in run.glyphs.iter() {
                let physical = glyph.physical((0., run.line_y), 1.0);
                let right_edge = (physical.x as f32 + glyph.w as f32).max(0.0_f32);
                width = width.max(right_edge);
            }
        }

        (width, height)
    }
}

impl Default for TextShaper {
    fn default() -> Self {
        Self::new()
    }
}
