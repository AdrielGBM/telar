//! The cache keys shaping, rasterization and COLR lookup are all addressed by.

use renderer_core::{Color, FontFamily, LineHeight, Raster, TextStyle, TextWrap};
use rustc_hash::FxHasher;
use std::hash::{Hash, Hasher};

/// Packs the shaping-relevant style axes into one `u32` so every cache key distinguishes e.g. bold from normal, or 2-line-clamped from unclamped, text of the same string — otherwise they would collide and one variant's glyphs/positions/truncation would be served for the other. Layout: `weight` bits 0-15, `italic` bit 16, `align` bits 17-18, `max_lines` bits 19-26 (0 = unlimited), `ellipsis` bit 27.
///
/// `font_family`/`line_height`/`letter_spacing`/`raster` do not fit that layout losslessly. With all four at their defaults the packed value is returned verbatim — keeping existing keys and the byte-exact software golden untouched — and anything else folds every axis into a full 32-bit hash. This keeps `text_style_bits`'s `u32` contract so callers across crates (e.g. the software renderer's pixmap cache) distinguish them for free, without any signature change.
pub fn text_style_bits(style: &TextStyle) -> u32 {
    let lines = style.clamp.max_lines().unwrap_or(0).min(255) as u32;
    let packed = (style.font_weight as u32)
        | ((style.font_style as u32) << 16)
        | ((style.text_align as u32) << 17)
        | (lines << 19)
        | ((style.clamp.ellipsis() as u32) << 27)
        | (((style.text_wrap == TextWrap::NoWrap) as u32) << 28);
    if style.font_family == FontFamily::SansSerif
        && style.line_height == LineHeight::Natural
        && style.letter_spacing == 0.0
        && style.raster == Raster::Smooth
    {
        return packed;
    }
    let mut h = FxHasher::default();
    packed.hash(&mut h);
    style.font_family.hash(&mut h);
    style.line_height.factor().map(f32::to_bits).hash(&mut h);
    style.letter_spacing.to_bits().hash(&mut h);
    style.raster.hash(&mut h);
    h.finish() as u32
}

/// One shaped run: every glyph's atlas key and its physical offset within the line box.
pub(super) type GlyphPositions = std::sync::Arc<Vec<(cosmic_text::CacheKey, i32, i32)>>;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
/// What a shaped paragraph is addressed by: its text, its style and the width it wrapped against.
pub struct ShapingCacheKey {
    pub text_hash: u64,
    pub font_size_bits: u32,
    pub width: u32,
    pub scale_factor_bits: u32,
    pub style_bits: u32,
    // Shaping is height-independent: it depends on wrap width, not container height.
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
/// What a rasterized string is addressed by: its shaping key plus the colour and scale it was drawn at.
pub struct TextCacheKey {
    pub text_hash: u64,
    pub font_size_bits: u32,
    pub width: u32,
    pub height: u32,
    pub color_packed: u32,
    pub style_bits: u32,
}

// `pub(super)`: hashed by the layout, raster and colr submodules to build their cache keys.
pub(super) fn hash_text(text: &str) -> u64 {
    let mut h = FxHasher::default();
    text.hash(&mut h);
    h.finish()
}

#[inline]
/// Builds the raster cache key for a string at a given style, colour and scale.
pub fn make_text_cache_key(
    text: &str,
    font_size: f32,
    width: u32,
    height: u32,
    color: Color,
    style_bits: u32,
) -> TextCacheKey {
    let rgba = color.to_rgba8();
    let color_packed = u32::from_le_bytes(rgba);
    TextCacheKey {
        text_hash: hash_text(text),
        font_size_bits: font_size.to_bits(),
        width,
        height,
        color_packed,
        style_bits,
    }
}
