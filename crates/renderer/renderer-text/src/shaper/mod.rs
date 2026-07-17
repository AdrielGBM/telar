use cosmic_text::{
    Align, Attrs, Buffer, CacheKey, FontSystem, Metrics, Shaping, Style, SwashCache, Weight,
};
use geometry_core::Rect;
use lru::LruCache;
use renderer_core::{TextAlign, TextStyle};
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
pub use cache::{TextCacheKey, make_text_cache_key, text_style_bits};
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
    // Keyed by (text_hash, max_width_bits, font_size_bits, style_bits); LRU-evicted at MEASURE_CACHE_CAP so a hot subset survives the cap instead of a full clear dropping everything.
    measure_cache: LruCache<(u64, u32, u32, u32), (f32, f32), FxBuildHasher>,
    // Whether a (text_hash, font_size_bits) shapes to any COLR glyph. Lets the software COLR fallback skip make_buffer + per-glyph get_image for plain UI text after the first evaluation. Symmetric with `blank_glyphs`.
    has_colr_cache: LruCache<(u64, u32, u32), bool, FxBuildHasher>,
    // Raw font bytes + face index, cached by font id so COLR rasterization does not re-read the font file on every atlas miss.
    colr_font_cache: FxHashMap<cosmic_text::fontdb::ID, Arc<(Vec<u8>, u32)>>,
    // Glyphs that swash cannot rasterize and that are not COLR glyphs either (e.g. whitespace); skipped on later frames so we do not re-attempt COLR rasterization for them every frame.
    blank_glyphs: FxHashSet<CacheKey>,
    // Cached real font metrics for the default sans-serif face; computed lazily on first request and reused across frames.
    font_metrics_cache: Option<renderer_core::FontMetrics>,
}

/// The buffer line height in pixels for `style`: `line_height` (a multiple of font size) when set, else
/// the natural `LINE_HEIGHT_FACTOR`. Shared by shaping and measuring so both reserve the same vertical space.
pub(crate) fn effective_line_height(style: &TextStyle) -> f32 {
    style.font_size * style.line_height.unwrap_or(LINE_HEIGHT_FACTOR)
}

fn cosmic_align(align: TextAlign) -> Option<Align> {
    match align {
        // Start keeps cosmic-text's default (left in LTR), so no explicit per-line align is set.
        TextAlign::Start => None,
        TextAlign::Center => Some(Align::Center),
        TextAlign::End => Some(Align::End),
        TextAlign::Justify => Some(Align::Justified),
    }
}

fn shape_buffer(font_system: &mut FontSystem, text: &str, rect: Rect, style: &TextStyle) -> Buffer {
    let font_size = style.font_size;
    let metrics = Metrics::new(font_size, effective_line_height(style));
    let mut buffer = Buffer::new(font_system, metrics);
    buffer.set_size(Some(rect.width), Some(rect.height));
    let mut attrs = Attrs::new()
        .weight(Weight(style.weight))
        .style(if style.italic {
            Style::Italic
        } else {
            Style::Normal
        });
    // Only set letter spacing when non-default so unspaced text keeps cosmic-text's exact default shaping (and the byte-golden).
    if style.letter_spacing != 0.0 {
        attrs = attrs.letter_spacing(style.letter_spacing);
    }
    buffer.set_text(text, &attrs, Shaping::Advanced, None);
    // Alignment shifts glyph x within the line box; applied before shaping so positions bake it in.
    if let Some(a) = cosmic_align(style.align) {
        for line in buffer.lines.iter_mut() {
            line.set_align(Some(a));
        }
    }
    buffer.shape_until_scroll(font_system, false);
    buffer
}

/// Shapes `text` into `rect`, then applies `max_lines`/`ellipsis` clamping: cosmic-text has no public
/// ellipsis, so a clamped overflow is truncated at the start of the first dropped visual line, and
/// (with ellipsis) `…` is appended and characters are dropped until it fits. Single logical line per
/// call is assumed for the cut offset (UI labels), which is the common clamp case.
fn make_buffer(font_system: &mut FontSystem, text: &str, rect: Rect, style: &TextStyle) -> Buffer {
    let buffer = shape_buffer(font_system, text, rect, style);
    let Some(max) = style.max_lines.map(usize::from).filter(|&n| n > 0) else {
        return buffer;
    };
    // Byte offset (within the single buffer line) where each visual line begins.
    let line_starts: Vec<usize> = buffer
        .layout_runs()
        .map(|run| run.glyphs.first().map(|g| g.start).unwrap_or(0))
        .collect();
    if line_starts.len() <= max {
        return buffer;
    }
    let cut = line_starts[max].min(text.len());
    let head = text[..cut].trim_end();
    if !style.ellipsis {
        return shape_buffer(font_system, head, rect, style);
    }
    // Ellipsis: append `…`, dropping trailing chars until the result fits in `max` lines.
    let mut end = head.len();
    loop {
        let candidate = format!("{}\u{2026}", &head[..end]);
        let b = shape_buffer(font_system, &candidate, rect, style);
        if b.layout_runs().count() <= max {
            return b;
        }
        if end == 0 {
            return b;
        }
        end -= 1;
        while end > 0 && !head.is_char_boundary(end) {
            end -= 1;
        }
        while end > 0 && head.as_bytes()[end - 1].is_ascii_whitespace() {
            end -= 1;
        }
    }
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

            let mut font_system = if needs_custom_db {
                let mut db = fontdb::Database::new();
                if let Some(ref dir) = font.system_fonts_dir {
                    db.load_fonts_dir(dir);
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
            };
            // Route the default (`Family::SansSerif`) face to the first configured candidate that resolves — the theme's chosen font family, or an OEM stack. Applied to whichever db built the system (custom dir or default system fonts), so a plain-desktop app still honors the family.
            let db = font_system.db_mut();
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
            font_system
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
