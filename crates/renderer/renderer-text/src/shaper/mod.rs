use cosmic_text::{
    Align, Attrs, Buffer, CacheKey, Color as CosmicColor, FontSystem, Metrics, Shaping, Style,
    SwashCache, Weight, fontdb,
};
use geometry_core::{Color, Rect};
use renderer_cache::{Cache, CacheStat, Policy, limits};
use renderer_core::{TextAlign, TextRun, TextStyle};
use rustc_hash::FxHashSet;
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

use cache::{GlyphPositions, ShapingCacheKey};

/// Line height as a multiple of font size, applied uniformly at layout time. Exposed so widgets that vertically place text (e.g. centering a button label) center against the exact box the renderer uses.
pub const LINE_HEIGHT_FACTOR: f32 = 1.2;

pub struct TextShaperConfig {
    /// Bounds the composed-string raster cache. Whole [`Policy`] rather than a byte count so an app can also say how
    /// long a raster may sit idle and whether one sighting is enough to keep it.
    pub raster: Policy,
    pub shaping: Policy,
    pub font: renderer_core::FontConfig,
}

impl Default for TextShaperConfig {
    fn default() -> Self {
        Self {
            raster: limits::TEXT_RASTER,
            shaping: limits::TEXT_SHAPING,
            font: renderer_core::FontConfig::default(),
        }
    }
}

pub struct TextShaper {
    font_system: FontSystem,
    swash_cache: SwashCache,
    // Written only by the hardware backend, which is the only caller of `layout_glyphs`. Costs nothing until a glyph is packed; see `GlyphAtlas::insert`.
    pub atlas: GlyphAtlas,
    // Composed strings as premultiplied RGBA. Admission-gated: a clock at `%H:%M:%S` mints a string a second that no later frame will ever ask for, and text worth keeping is text something asked for twice.
    raster_cache: Cache<TextCacheKey, Arc<[u8]>>,
    shaping_cache: Cache<ShapingCacheKey, GlyphPositions>,
    measure_cache: Cache<(u64, u32, u32, u32), (f32, f32)>,
    // Whether a (text_hash, font_size_bits) shapes to any COLR glyph. Lets the software COLR fallback skip make_buffer + per-glyph get_image for plain UI text after the first evaluation. Symmetric with `blank_glyphs`.
    has_colr_cache: Cache<(u64, u32, u32), bool>,
    // Raw font bytes + face index, so COLR rasterization does not re-read the font file on every atlas miss.
    colr_font_cache: Cache<fontdb::ID, Arc<(Vec<u8>, u32)>>,
    // Glyphs that swash cannot rasterize and that are not COLR glyphs either (e.g. whitespace); skipped on later frames so we do not re-attempt COLR rasterization for them every frame. Left unbounded where the caches above are not: an entry is one `CacheKey`, the set is bounded by the glyph repertoire actually attempted, and evicting one buys back 16 bytes in exchange for re-running a COLR rasterization that already failed.
    blank_glyphs: FxHashSet<CacheKey>,
    // Cached real font metrics for the default sans-serif face; computed lazily on first request and reused across frames.
    font_metrics_cache: Option<renderer_core::FontMetrics>,
}

/// The buffer line height in pixels for `style`: `line_height` (a multiple of font size) when set, else the natural `LINE_HEIGHT_FACTOR`. Shared by shaping and measuring so both reserve the same vertical space.
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

/// Shapes `text` into `rect`, then applies `max_lines`/`ellipsis` clamping: cosmic-text has no public ellipsis, so a clamped overflow is truncated at the start of the first dropped visual line, and (with ellipsis) `…` is appended and characters are dropped until it fits. Single logical line per call is assumed for the cut offset (UI labels), which is the common clamp case.
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

/// Shapes a rich-text paragraph (`runs`) into `rect` using the base metrics, giving each run its own weight, slant, and colour via a per-span `Attrs`. `max_lines`/`ellipsis` are not applied here — cosmic-text has no cross-run truncation, so the caller clamps by visual line instead.
pub(crate) fn make_buffer_rich(
    font_system: &mut FontSystem,
    runs: &[TextRun],
    rect: Rect,
    base: &TextStyle,
) -> Buffer {
    let metrics = Metrics::new(base.font_size, effective_line_height(base));
    let mut buffer = Buffer::new(font_system, metrics);
    buffer.set_size(Some(rect.width), Some(rect.height));
    let spans = runs.iter().map(|run| {
        let mut attrs = Attrs::new()
            .weight(Weight(run.weight))
            .style(if run.italic {
                Style::Italic
            } else {
                Style::Normal
            })
            .color(to_cosmic_color(run.color));
        if base.letter_spacing != 0.0 {
            attrs = attrs.letter_spacing(base.letter_spacing);
        }
        (run.text.as_ref(), attrs)
    });
    buffer.set_rich_text(
        spans,
        &Attrs::new(),
        Shaping::Advanced,
        cosmic_align(base.align),
    );
    buffer.shape_until_scroll(font_system, false);
    buffer
}

fn to_cosmic_color(color: Color) -> CosmicColor {
    let [r, g, b, a] = color.to_rgba8();
    CosmicColor::rgba(r, g, b, a)
}

pub(crate) fn from_cosmic_color(color: CosmicColor) -> Color {
    Color::rgba(
        color.r() as f32 / 255.0,
        color.g() as f32 / 255.0,
        color.b() as f32 / 255.0,
        color.a() as f32 / 255.0,
    )
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
            raster_cache: Cache::new(config.raster, |pixels| pixels.len()),
            shaping_cache: Cache::new(config.shaping, |positions| {
                positions
                    .len()
                    .saturating_mul(size_of::<(CacheKey, i32, i32)>())
            }),
            measure_cache: Cache::new(limits::TEXT_MEASURE, |_| limits::SMALL_ENTRY_BYTES),
            has_colr_cache: Cache::new(limits::TEXT_HAS_COLR, |_| limits::SMALL_ENTRY_BYTES),
            colr_font_cache: Cache::new(limits::FONT_FILE, |font| font.0.len()),
            blank_glyphs: FxHashSet::default(),
            font_metrics_cache: None,
        }
    }

    /// Drops every raster, shaped run and measurement nothing has asked for within its idle horizon, and the glyph
    /// masks if they have outgrown their ceiling.
    ///
    /// The caches sweep themselves as they are used, which covers a running shell but not one that has stopped
    /// drawing — with no frames there are no accesses, and no access is a chance to reclaim. Call this when the app
    /// knows it has gone quiet.
    pub fn sweep_idle(&mut self) {
        self.raster_cache.sweep();
        self.shaping_cache.sweep();
        self.measure_cache.sweep();
        self.has_colr_cache.sweep();
        self.colr_font_cache.sweep();
        self.trim_glyph_rasters();
    }

    /// What every cache this shaper owns is holding, for a census something outside the renderer can read.
    ///
    /// Counted from the caches themselves on each call, never tracked alongside them, so no number here can drift
    /// from what is true. The atlas and `swash_cache` are measured by hand — neither is a [`Cache`] — and the atlas
    /// reads zero on a software-only process, which is the point of it being lazy.
    pub fn cache_stats(&self) -> Vec<CacheStat> {
        let glyph_bytes: usize = self
            .swash_cache
            .image_cache
            .values()
            .flatten()
            .map(|image| image.data.len())
            .sum();
        vec![
            self.raster_cache.stat("text.raster"),
            self.shaping_cache.stat("text.shaping"),
            self.measure_cache.stat("text.measure"),
            self.has_colr_cache.stat("text.has_colr"),
            self.colr_font_cache.stat("text.font_file"),
            CacheStat {
                name: "text.glyph_raster",
                bytes: glyph_bytes,
                entries: self.swash_cache.image_cache.len(),
                capacity: limits::GLYPH_RASTER_BUDGET_BYTES,
            },
            CacheStat {
                name: "text.atlas",
                // What the glyphs have written, not the plane they were written into: the plane is `mmap`'d zero pages that cost nothing until touched, and counting it made the census claim more memory than the whole process had.
                bytes: self.atlas.packed_bytes(),
                entries: self.atlas.glyph_count(),
                capacity: self.atlas.reserved_bytes(),
            },
        ]
    }

    /// Empties cosmic-text's glyph caches once the rasterized masks pass [`limits::GLYPH_RASTER_BUDGET_BYTES`].
    ///
    /// `SwashCache` is two `HashMap`s cosmic-text never evicts from. The hardware backend prunes it in passing —
    /// packing a glyph into a full atlas removes the evicted key — but the software backend, which reaches it
    /// through `Buffer::draw`, has nothing that ever removes anything. Left alone it grows with every distinct
    /// (glyph, size, subpixel bin) a session ever draws.
    ///
    /// All or nothing, because cosmic-text records no recency: with no way to tell a hot glyph from a cold one,
    /// evicting a chosen few would be guessing. Clearing costs re-rasterizing the glyphs still in use, which is why
    /// the ceiling sits far above what a shell actually holds and why this only ever runs from an idle sweep.
    fn trim_glyph_rasters(&mut self) {
        let resident: usize = self
            .swash_cache
            .image_cache
            .values()
            .flatten()
            .map(|image| image.data.len())
            .sum();
        if resident > limits::GLYPH_RASTER_BUDGET_BYTES {
            self.swash_cache.image_cache.clear();
            self.swash_cache.outline_command_cache.clear();
        }
    }
}

impl Default for TextShaper {
    fn default() -> Self {
        Self::new()
    }
}
