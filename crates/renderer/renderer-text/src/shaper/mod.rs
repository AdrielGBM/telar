use cosmic_text::{Attrs, Buffer, CacheKey, FontSystem, Metrics, Shaping, SwashCache};
use geometry_core::Rect;
use lru::LruCache;
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use std::num::NonZeroUsize;
use std::sync::Arc;

mod atlas;
mod cache;
mod colr;
mod layout;
mod metrics;
mod raster;
#[cfg(test)]
mod tests;

pub use atlas::{ATLAS_SIZE, GlyphAtlas, GlyphInfo};
pub use cache::{TextCacheKey, make_text_cache_key};
pub use colr::ColrGlyph;

use cache::{AlphaCacheKey, AlphaCacheScale, PixelCacheScale, ShapingCacheKey, ShapingCacheScale};
use clru::{CLruCache, CLruCacheConfig};

/// Line height as a multiple of font size, applied uniformly at layout time. Exposed so widgets that
/// vertically place text (e.g. centering a button label) center against the exact box the renderer uses.
pub const LINE_HEIGHT_FACTOR: f32 = 1.2;

// Entry caps for the measure and has-COLR caches. Values are tiny (a few bytes each), so the cap trades negligible memory for a high hit rate.
const MEASURE_CACHE_CAP: usize = 1000;
const HAS_COLR_CACHE_CAP: usize = 1000;

pub struct TextShaperConfig {
    pub pixel_cache_budget_bytes: usize,
    pub alpha_cache_budget_bytes: usize,
    pub shaping_cache_budget_bytes: usize,
    pub font: renderer_core::FontConfig,
}

impl Default for TextShaperConfig {
    fn default() -> Self {
        Self {
            pixel_cache_budget_bytes: 64 * 1024 * 1024,
            alpha_cache_budget_bytes: 64 * 1024 * 1024,
            shaping_cache_budget_bytes: 24 * 1024 * 1024,
            font: renderer_core::FontConfig::default(),
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
    // Keyed by (text_hash, max_width_bits, font_size_bits); LRU-evicted at MEASURE_CACHE_CAP so a hot subset survives the cap instead of a full clear dropping everything.
    measure_cache: LruCache<(u64, u32, u32), (f32, f32), FxBuildHasher>,
    // Whether a (text_hash, font_size_bits) shapes to any COLR glyph. Lets the software COLR fallback skip make_buffer + per-glyph get_image for plain UI text after the first evaluation. Symmetric with `blank_glyphs`.
    has_colr_cache: LruCache<(u64, u32), bool, FxBuildHasher>,
    // Raw font bytes + face index, cached by font id so COLR rasterization does not re-read the font file on every atlas miss.
    colr_font_cache: FxHashMap<cosmic_text::fontdb::ID, Arc<(Vec<u8>, u32)>>,
    // Glyphs that swash cannot rasterize and that are not COLR glyphs either (e.g. whitespace); skipped on later frames so we do not re-attempt COLR rasterization for them every frame.
    blank_glyphs: FxHashSet<CacheKey>,
    // Cached real font metrics for the default sans-serif face; computed lazily on first request and reused across frames.
    font_metrics_cache: Option<renderer_core::FontMetrics>,
}

fn make_buffer(font_system: &mut FontSystem, text: &str, rect: Rect, font_size: f32) -> Buffer {
    let metrics = Metrics::new(font_size, font_size * LINE_HEIGHT_FACTOR);
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
        let font_system = {
            let font = config.font;
            let needs_custom_db = font.system_fonts_dir.is_some()
                || !font.extra_font_paths.is_empty()
                || !font.font_data.is_empty();

            if needs_custom_db {
                let mut db = fontdb::Database::new();
                if let Some(ref dir) = font.system_fonts_dir {
                    db.load_fonts_dir(dir);
                    for name in &font.sans_serif_family_candidates {
                        if db
                            .query(&fontdb::Query {
                                families: &[fontdb::Family::Name(name)],
                                ..fontdb::Query::default()
                            })
                            .is_some()
                        {
                            db.set_sans_serif_family(name.as_str());
                            break;
                        }
                    }
                } else {
                    db.load_system_fonts();
                }
                for path in &font.extra_font_paths {
                    db.load_font_file(path).ok();
                }
                for data in font.font_data {
                    db.load_font_data(data);
                }
                let locale = std::env::var("LANG").unwrap_or_else(|_| "en-US".to_string());
                FontSystem::new_with_locale_and_db(locale, db)
            } else {
                FontSystem::new()
            }
        };
        Self {
            font_system,
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
            measure_cache: LruCache::with_hasher(
                NonZeroUsize::new(MEASURE_CACHE_CAP).unwrap(),
                FxBuildHasher,
            ),
            has_colr_cache: LruCache::with_hasher(
                NonZeroUsize::new(HAS_COLR_CACHE_CAP).unwrap(),
                FxBuildHasher,
            ),
            colr_font_cache: FxHashMap::default(),
            blank_glyphs: FxHashSet::default(),
            font_metrics_cache: None,
        }
    }
}

impl Default for TextShaper {
    fn default() -> Self {
        Self::new()
    }
}
