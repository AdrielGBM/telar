//! The glyph atlas: packing rasterized glyphs into one texture and evicting when it fills.

use cosmic_text::CacheKey;
use etagere::{AllocId, BucketedAtlasAllocator, size2};
use lru::LruCache;
use rustc_hash::FxHashMap;

#[derive(Clone, Copy, Debug)]
/// Where one glyph sits in the atlas texture, and how big it is.
pub struct AtlasEntry {
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
    pub glyph_width: u32,
    pub glyph_height: u32,
    pub placement_left: i32,
    pub placement_top: i32,
    pub is_color_glyph: bool,
}

/// A positioned glyph: its atlas entry and where it is drawn.
pub struct GlyphInfo {
    pub dest_rect: [f32; 4],
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
    pub color: [f32; 4],
}

/// The atlas texture's side, in pixels.
pub const ATLAS_SIZE: u32 = 2048;

/// The shared glyph texture: packing rasterized glyphs in, and evicting when it fills.
pub struct GlyphAtlas {
    pub pixels: Vec<u8>,
    pub dirty_rects: Vec<[u32; 4]>,
    entries: FxHashMap<CacheKey, AtlasEntry>,
    allocator: BucketedAtlasAllocator,
    lru_cache: LruCache<CacheKey, AllocId>,
}

impl GlyphAtlas {
    /// Opens an atlas that has not reserved its pixels yet. See `insert` for why they wait.
    pub fn new() -> Self {
        Self {
            pixels: Vec::new(),
            dirty_rects: Vec::new(),
            entries: FxHashMap::default(),
            allocator: BucketedAtlasAllocator::new(size2(ATLAS_SIZE as i32, ATLAS_SIZE as i32)),
            lru_cache: LruCache::unbounded(),
        }
    }

    pub fn drain_dirty_rects(&mut self) -> std::vec::Drain<'_, [u32; 4]> {
        self.dirty_rects.drain(..)
    }

    /// Bytes of address space the atlas has reserved: zero until the first glyph is packed, the full plane after.
    ///
    /// Reserved, not resident, and on a sparse atlas the two differ by orders of magnitude. `vec![0u8; N]` at this size goes to `mmap`, which hands back zero pages that cost nothing until something writes to them — an atlas holding a few hundred glyphs measures 16 MiB here and a fraction of one in `/proc/self/smaps`.
    pub fn reserved_bytes(&self) -> usize {
        self.pixels.len()
    }

    /// Bytes the packed glyphs have actually written, which is what the atlas costs in memory.
    ///
    /// Reported instead of [`Self::reserved_bytes`] wherever the figure sits beside other caches' real sizes: an atlas that reserves 16 MiB and has written 300 KB is a 300 KB cost, and summing the reservation with genuine allocations produced a census claiming more memory than the whole process had.
    pub fn packed_bytes(&self) -> usize {
        self.entries
            .values()
            .map(|entry| (entry.glyph_width as usize) * (entry.glyph_height as usize) * 4)
            .sum()
    }

    pub fn glyph_count(&self) -> usize {
        self.entries.len()
    }

    // `pub(super)` for `shaper::layout`, which packs newly rasterized glyphs.
    /// Packs one rasterized glyph, reserving the atlas's 16 MiB on the first glyph to arrive.
    ///
    /// Lazily, because only the hardware backend ever packs a glyph — `layout_glyphs` is the sole path in, and only `renderer-hardware` calls it. The software backend composites text through `rasterize`, never touching the atlas, yet it builds a [`TextShaper`](super::TextShaper) like everyone else; reserving up front billed every software surface 16 MiB of zeroed RGBA it would never read, which was the largest single line in a heap profile of a shell that has no GPU backend at all.
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
        if self.pixels.is_empty() {
            self.pixels = vec![0u8; (ATLAS_SIZE * ATLAS_SIZE * 4) as usize];
        }
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
        // `get` promotes to MRU; the returned value is discarded, since only the side effect matters.
        self.lru_cache.get(key)?;
        self.entries.get(key).copied()
    }

    // `pub(super)` for `shaper::layout`, which calls it to make room when the atlas is full.
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
