pub mod colr;
mod measure;
mod shaper;

pub use measure::{
    ShaperMetrics, font_family_available, measure_ink_bounds, measure_rich_text, measure_text,
    set_measure_font_config,
};
pub use shaper::{
    ATLAS_SIZE, ColrGlyph, GlyphAtlas, GlyphInfo, LINE_HEIGHT_FACTOR, TextCacheKey, TextShaper,
    TextShaperConfig, make_text_cache_key, text_style_bits,
};

/// Re-export of fontdb::ID so consumers of this crate do not need a direct cosmic-text dependency.
pub use cosmic_text::fontdb;
