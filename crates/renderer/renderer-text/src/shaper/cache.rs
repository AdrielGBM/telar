//! The cache keys shaping, rasterization and COLR lookup are all addressed by.

use renderer_core::{Color, FontFamily, LineHeight, Raster, TextCase, TextStyle, TextWrap};
use rustc_hash::FxHasher;
use std::hash::{Hash, Hasher};

#[derive(Clone, Copy)]
struct BitField {
    shift: u32,
    width: u32,
}

impl BitField {
    const fn mask(self) -> u32 {
        ((1u64 << self.width) - 1) as u32
    }

    fn put(self, value: u32) -> u32 {
        assert!(value <= self.mask(), "style value overflows its bit field");
        value << self.shift
    }
}

const WEIGHT: BitField = BitField {
    shift: 0,
    width: 16,
};
const TEXT_ALIGN: BitField = BitField {
    shift: 17,
    width: 2,
};
const MAX_LINES: BitField = BitField {
    shift: 19,
    width: 8,
};
const ELLIPSIS: BitField = BitField {
    shift: 27,
    width: 1,
};
const NO_WRAP: BitField = BitField {
    shift: 28,
    width: 1,
};
const FONT_STYLE: BitField = BitField {
    shift: 29,
    width: 2,
};

const FIELDS: [BitField; 6] = [WEIGHT, TEXT_ALIGN, MAX_LINES, ELLIPSIS, NO_WRAP, FONT_STYLE];

const _: () = {
    let mut claimed = 0u32;
    let mut i = 0;
    while i < FIELDS.len() {
        let field = FIELDS[i];
        assert!(field.shift + field.width <= 32);
        let bits = field.mask() << field.shift;
        assert!(claimed & bits == 0, "style bit fields overlap");
        claimed |= bits;
        i += 1;
    }
};

/// Packs the shaping-relevant style axes into one `u32` so every cache key tells apart, say, bold from normal or a 2-line clamp from none. Each axis owns the bit range of its `BitField` const above; `max_lines` of 0 means unlimited.
///
/// `font_family`/`line_height`/`letter_spacing`/`raster` do not fit that layout losslessly. With all of them at their defaults the packed value is returned verbatim, and anything else folds every axis into a 32-bit hash.
pub fn text_style_bits(style: &TextStyle) -> u32 {
    let lines = style.clamp.max_lines().unwrap_or(0).min(255) as u32;
    let packed = WEIGHT.put(super::drawn_weight(style) as u32)
        | FONT_STYLE.put(style.font_style as u32)
        | TEXT_ALIGN.put(style.text_align as u32)
        | MAX_LINES.put(lines)
        | ELLIPSIS.put(style.clamp.ellipsis() as u32)
        | NO_WRAP.put((style.text_wrap == TextWrap::NoWrap) as u32);
    if style.font_family == FontFamily::SansSerif
        && style.line_height == LineHeight::Natural
        && style.letter_spacing == 0.0
        && style.raster == Raster::Smooth
        && style.font_variations.is_empty()
        && style.font_features.is_empty()
        && style.text_case == TextCase::AsWritten
    {
        return packed;
    }
    let mut h = FxHasher::default();
    packed.hash(&mut h);
    style.font_family.hash(&mut h);
    style.line_height.factor().map(f32::to_bits).hash(&mut h);
    style.letter_spacing.to_bits().hash(&mut h);
    style.raster.hash(&mut h);
    style.font_variations.key().hash(&mut h);
    style.font_features.key().hash(&mut h);
    if style.text_case != TextCase::AsWritten {
        style.text_case.hash(&mut h);
        style.lang.hash(&mut h);
    }
    h.finish() as u32
}

/// One shaped run: every glyph's atlas key and its physical offset within the line box.
pub(super) type GlyphPositions = std::sync::Arc<Vec<(cosmic_text::CacheKey, i32, i32)>>;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
/// What a shaped paragraph is addressed by: its text, its style, the exact width it wrapped against and how many lines its height shows.
pub struct ShapingCacheKey {
    pub text_hash: u64,
    pub font_size_bits: u32,
    pub width_bits: u32,
    pub visible_lines: u32,
    pub scale_factor_bits: u32,
    pub style_bits: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
/// What a rasterized string is addressed by: its shaping key plus the colour and scale it was drawn at.
pub struct TextCacheKey {
    pub text_hash: u64,
    pub font_size_bits: u32,
    pub width_bits: u32,
    pub height_bits: u32,
    pub color_packed: u32,
    pub style_bits: u32,
}

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
    width: f32,
    height: f32,
    color: Color,
    style_bits: u32,
) -> TextCacheKey {
    let rgba = color.to_rgba8();
    let color_packed = u32::from_le_bytes(rgba);
    TextCacheKey {
        text_hash: hash_text(text),
        font_size_bits: font_size.to_bits(),
        width_bits: width.to_bits(),
        height_bits: height.to_bits(),
        color_packed,
        style_bits,
    }
}

pub(super) fn visible_lines(height: f32, style: &TextStyle) -> u32 {
    let line_height = super::effective_line_height(style);
    if !(line_height > 0.0 && height > 0.0) {
        return 0;
    }
    (height / line_height).ceil() as u32
}

pub(super) fn line_snapped_height(visible_lines: u32, style: &TextStyle) -> f32 {
    visible_lines as f32 * super::effective_line_height(style)
}

#[cfg(test)]
#[path = "cache.test.rs"]
mod tests;
