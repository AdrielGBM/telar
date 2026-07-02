use cosmic_text::CacheKey;
use etagere::{AllocId, BucketedAtlasAllocator, size2};
use lru::LruCache;
use rustc_hash::FxHashMap;

#[derive(Clone, Copy, Debug)]
pub struct AtlasEntry {
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
    pub glyph_width: u32,
    pub glyph_height: u32,
    pub placement_left: i32,
    pub placement_top: i32,
    pub is_color_glyph: bool,
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
    entries: FxHashMap<CacheKey, AtlasEntry>,
    allocator: BucketedAtlasAllocator,
    lru_cache: LruCache<CacheKey, AllocId>,
}

impl GlyphAtlas {
    pub fn new() -> Self {
        Self {
            pixels: vec![0u8; (ATLAS_SIZE * ATLAS_SIZE * 4) as usize],
            dirty_rects: Vec::new(),
            entries: FxHashMap::default(),
            allocator: BucketedAtlasAllocator::new(size2(ATLAS_SIZE as i32, ATLAS_SIZE as i32)),
            lru_cache: LruCache::unbounded(),
        }
    }

    pub fn drain_dirty_rects(&mut self) -> std::vec::Drain<'_, [u32; 4]> {
        self.dirty_rects.drain(..)
    }

    // pub(super): called from `shaper::layout`, a sibling module, when packing newly rasterized glyphs.
    pub(super) fn insert(
        &mut self,
        key: CacheKey,
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
        };
        self.entries.insert(key, entry);
        self.lru_cache.put(key, alloc_id);
        self.dirty_rects.push([ax, ay, w, h]);
        Some(entry)
    }

    pub fn fetch(&mut self, key: &CacheKey) -> Option<AtlasEntry> {
        // lru_cache.get promotes to MRU; returned value is discarded — only the side effect matters.
        self.lru_cache.get(key)?;
        self.entries.get(key).copied()
    }

    // pub(super): called from `shaper::layout`, a sibling module, to make room when the atlas is full.
    pub(super) fn evict_lru(&mut self) -> Option<CacheKey> {
        let (key, alloc_id) = self.lru_cache.pop_lru()?;
        self.allocator.deallocate(alloc_id);
        self.entries.remove(&key);
        Some(key)
    }
}

impl Default for GlyphAtlas {
    fn default() -> Self {
        Self::new()
    }
}
