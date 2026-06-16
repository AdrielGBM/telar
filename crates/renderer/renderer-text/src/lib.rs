pub mod colr;
mod shaper;

pub use shaper::{
    ATLAS_SIZE, ColrGlyph, GlyphAtlas, GlyphInfo, TextCacheKey, TextShaper, TextShaperConfig,
    make_text_cache_key,
};

/// Re-export of fontdb::ID so consumers of this crate do not need a direct cosmic-text dependency.
pub use cosmic_text::fontdb;
