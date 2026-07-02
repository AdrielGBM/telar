use clru::WeightScale;
use cosmic_text::CacheKey;
use renderer_core::Color;
use rustc_hash::FxHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ShapingCacheKey {
    pub text_hash: u64,
    pub font_size_bits: u32,
    pub width: u32,
    pub scale_factor_bits: u32,
    // shaping is height-independent: it depends only on wrap width, not container height
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TextCacheKey {
    pub text_hash: u64,
    pub font_size_bits: u32,
    pub width: u32,
    pub height: u32,
    pub color_packed: u32,
}

// pub(super): constructed from sibling submodules (layout, raster) that need the shaper's private cache-key types.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct AlphaCacheKey {
    pub(super) text_hash: u64,
    pub(super) font_size_bits: u32,
    pub(super) width: u32,
    pub(super) height: u32,
}

// pub(super): hashed by layout/raster/colr submodules to build their respective cache keys.
pub(super) fn hash_text(text: &str) -> u64 {
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

// pub(super): referenced as field types by TextShaper in the parent `shaper` module.
pub(super) struct PixelCacheScale;
impl WeightScale<TextCacheKey, Arc<[u8]>> for PixelCacheScale {
    fn weight(&self, _key: &TextCacheKey, value: &Arc<[u8]>) -> usize {
        value.len().max(1)
    }
}

pub(super) struct AlphaCacheScale;
impl WeightScale<AlphaCacheKey, Arc<[u8]>> for AlphaCacheScale {
    fn weight(&self, _key: &AlphaCacheKey, value: &Arc<[u8]>) -> usize {
        value.len().max(1)
    }
}

pub(super) struct ShapingCacheScale;
impl WeightScale<ShapingCacheKey, Arc<Vec<(CacheKey, i32, i32)>>> for ShapingCacheScale {
    fn weight(&self, _key: &ShapingCacheKey, value: &Arc<Vec<(CacheKey, i32, i32)>>) -> usize {
        value.len().saturating_mul(24).max(1)
    }
}
