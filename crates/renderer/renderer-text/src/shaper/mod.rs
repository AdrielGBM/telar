//! [`TextShaper`]: the per-thread shaper, its caches, and the atlas every renderer on the thread shares.

use cosmic_text::{
    Align, Attrs, Buffer, CacheKey, CacheKeyFlags, Color as CosmicColor, Family, FontSystem,
    LayoutGlyph, Metrics, PhysicalGlyph, Shaping, Style, SwashCache, Weight, fontdb,
};
use geometry_core::{Color, Rect};
use renderer_cache::{Cache, CacheStat, Policy, limits};
use renderer_core::{FontFamily, FontStyle, Paint, Raster, Span, TextAlign, TextStyle, TextWrap};
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

use crate::fonts::{self, Fonts};

use cache::{GlyphPositions, ShapingCacheKey};

/// Line height as a multiple of font size, applied uniformly at layout time. Exposed so widgets that vertically place text (e.g. centering a button label) center against the exact box the renderer uses.
pub const LINE_HEIGHT_FACTOR: f32 = 1.2;

/// The fonts and cache bounds a shaper is built with.
pub struct TextShaperConfig {
    /// Bounds the composed-string raster cache. Whole [`Policy`] rather than a byte count so an app can also say how long a raster may sit idle and whether one sighting is enough to keep it.
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

/// The per-thread shaper: cosmic-text, its caches, and the atlas every renderer on the thread samples.
pub struct TextShaper {
    font_system: FontSystem,
    swash_cache: SwashCache,
    // Written only by the hardware backend, the only caller of `layout_glyphs`. Costs nothing until a glyph is packed.
    pub atlas: GlyphAtlas,
    // Composed strings as premultiplied RGBA. Admission-gated: a clock at `%H:%M:%S` mints a string a second no later frame will ask for, and text worth keeping is text something asked for twice.
    raster_cache: Cache<TextCacheKey, Arc<[u8]>>,
    shaping_cache: Cache<ShapingCacheKey, GlyphPositions>,
    measure_cache: Cache<(u64, u32, u32, u32), (f32, f32)>,
    // Lets the software COLR fallback skip the buffer build and per-glyph lookup for plain UI text after the first evaluation. Symmetric with `blank_glyphs`.
    has_colr_cache: Cache<(u64, u32, u32), bool>,
    // So COLR rasterization does not re-read the font file on every atlas miss.
    colr_font_cache: Cache<fontdb::ID, Arc<(Vec<u8>, u32)>>,
    // Glyphs swash cannot rasterize that are not COLR either, skipped on later frames. Left unbounded, unlike the caches above: an entry is one `CacheKey`, the set is bounded by the glyph repertoire attempted, and evicting one buys 16 bytes in exchange for re-running a rasterization that already failed.
    blank_glyphs: FxHashSet<CacheKey>,
    // Computed lazily on first request and reused across frames.
    font_metrics_cache: Option<renderer_core::FontMetrics>,
    // Whether a `Named` family in a `Stack` has an installed face — a `fontdb::Database::query` per distinct name rather than per shape call. Unbounded like `blank_glyphs`, for the same reason: the set is bounded by the family names an application actually names, not by how often it shapes.
    family_availability: rustc_hash::FxHashMap<Arc<str>, bool>,
}

/// The buffer line height in pixels for `style`: `line_height` (a multiple of font size) when set, else the natural `LINE_HEIGHT_FACTOR`. Shared by shaping and measuring so both reserve the same vertical space.
pub(crate) fn effective_line_height(style: &TextStyle) -> f32 {
    style.font_size * style.line_height.factor().unwrap_or(LINE_HEIGHT_FACTOR)
}

/// The cache-key flags `raster` implies. `PIXEL_FONT` makes swash round the fractional offset it bakes into the glyph image, and — because it rides in the [`CacheKey`] — keeps a pixel-grid glyph from sharing an atlas slot with the smooth raster of the same glyph at the same size.
fn raster_flags(raster: Raster) -> CacheKeyFlags {
    match raster {
        Raster::Smooth => CacheKeyFlags::empty(),
        Raster::Pixel => CacheKeyFlags::PIXEL_FONT,
    }
}

/// Where one shaped glyph lands, on the grid `raster` asks for.
///
/// [`LayoutGlyph::physical`] bins the fractional x into a quarter-pixel offset the rasterizer bakes into the glyph image. Putting a glyph on a whole pixel therefore means rounding the position *before* it is binned, not moving the result: the offset is part of the cache key, so the same glyph at x.25 and at x.0 are two different rasters rather than one raster in two places.
pub(crate) fn physical_glyph(
    glyph: &LayoutGlyph,
    offset: (f32, f32),
    scale: f32,
    raster: Raster,
) -> PhysicalGlyph {
    if raster == Raster::Smooth {
        return glyph.physical(offset, scale);
    }
    let x_offset = glyph.font_size * glyph.x_offset;
    let y_offset = glyph.font_size * glyph.y_offset;
    let (cache_key, x, y) = CacheKey::new(
        glyph.font_id,
        glyph.glyph_id,
        glyph.font_size * scale,
        (
            (glyph.x + x_offset).mul_add(scale, offset.0).round(),
            (glyph.y - y_offset).mul_add(scale, offset.1).round(),
        ),
        glyph.font_weight,
        glyph.cache_key_flags,
    );
    PhysicalGlyph { cache_key, x, y }
}

/// Coverage at or above this is ink, below it is background. Half, because a pixel the outline covers more than half of is one the artist would have filled.
const PIXEL_COVERAGE_THRESHOLD: u8 = 128;

/// Resolves one glyph pixel's coverage under `raster`: blended as the rasterizer produced it, or on/off.
pub(crate) fn resolve_coverage(alpha: u8, raster: Raster) -> u8 {
    match raster {
        Raster::Smooth => alpha,
        Raster::Pixel if alpha >= PIXEL_COVERAGE_THRESHOLD => u8::MAX,
        Raster::Pixel => 0,
    }
}

fn cosmic_align(align: TextAlign) -> Option<Align> {
    match align {
        // Start keeps cosmic-text's default, so no explicit per-line align is set.
        TextAlign::Start => None,
        TextAlign::Center => Some(Align::Center),
        TextAlign::End => Some(Align::End),
        TextAlign::Justify => Some(Align::Justified),
    }
}

/// The cosmic-text attributes one resolved style asks for.
fn text_attrs<'a>(
    style: &'a TextStyle,
    font_system: &mut FontSystem,
    family_availability: &mut rustc_hash::FxHashMap<Arc<str>, bool>,
) -> Attrs<'a> {
    let mut attrs = Attrs::new()
        .family(resolve_cosmic_family(
            &style.font_family,
            font_system,
            family_availability,
        ))
        .weight(Weight(style.font_weight))
        .style(cosmic_style(style.font_style));
    // Only when non-default, so unspaced text keeps cosmic-text's exact default shaping and the byte-golden.
    if style.letter_spacing != 0.0 {
        attrs = attrs.letter_spacing(style.letter_spacing);
    }
    if style.raster != Raster::Smooth {
        attrs = attrs.cache_key_flags(raster_flags(style.raster));
    }
    attrs
}

/// The paragraph cut into runs of one style each: every span in order, with the paragraph's own style filling the gaps between them. Spans are expected sorted and non-overlapping; anything reaching backwards or past the end is clamped rather than trusted, since they are byte offsets that outlived the text they indexed.
fn styled_runs<'a>(text: &'a str, spans: &[Span], style: &TextStyle) -> Vec<(&'a str, TextStyle)> {
    let mut runs = Vec::with_capacity(spans.len() * 2 + 1);
    let mut at = 0usize;
    for span in spans {
        let start = (span.range.start as usize).clamp(at, text.len());
        let end = (span.range.end as usize).clamp(start, text.len());
        if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            continue;
        }
        if start > at {
            runs.push((&text[at..start], style.clone()));
        }
        if end > start {
            runs.push((&text[start..end], span.over.over(style)));
        }
        at = end;
    }
    if at < text.len() {
        runs.push((&text[at..], style.clone()));
    }
    runs
}

fn shape_buffer(
    font_system: &mut FontSystem,
    family_availability: &mut rustc_hash::FxHashMap<Arc<str>, bool>,
    text: &str,
    spans: Option<&[Span]>,
    rect: Rect,
    style: &TextStyle,
) -> Buffer {
    let font_size = style.font_size;
    let metrics = Metrics::new(font_size, effective_line_height(style));
    let mut buffer = Buffer::new(font_system, metrics);
    // `None` width is cosmic-text for "do not wrap": the line grows past the box instead of breaking, which is what a label that is really a token wants.
    let wrap_width = (!(style.text_wrap == TextWrap::NoWrap)).then_some(rect.width);
    buffer.set_size(wrap_width, Some(rect.height));
    let attrs = text_attrs(style, font_system, family_availability);
    // `set_text` rather than a one-span `set_rich_text`: the same call underneath, but this is the one the byte-exact software golden was recorded against.
    match spans.filter(|s| !s.is_empty()) {
        None => buffer.set_text(text, &attrs, Shaping::Advanced, None),
        Some(spans) => {
            let runs = styled_runs(text, spans, style);
            // A loop rather than `.iter().map(...).collect()`: each run resolves its family against the font database through the same `&mut` cache, which a closure re-invoked per item cannot reborrow as readily as a plain loop body can.
            let mut resolved: Vec<(&str, Attrs<'_>)> = Vec::with_capacity(runs.len());
            for (slice, run_style) in runs.iter() {
                let mut run_attrs = text_attrs(run_style, font_system, family_availability);
                if run_style.font_size != style.font_size {
                    run_attrs = run_attrs.metrics(Metrics::new(
                        run_style.font_size,
                        effective_line_height(run_style),
                    ));
                }
                if let Paint::Solid(color) = run_style.color {
                    run_attrs = run_attrs.color(to_cosmic_color(color));
                }
                resolved.push((*slice, run_attrs));
            }
            buffer.set_rich_text(resolved, &attrs, Shaping::Advanced, None);
        }
    }
    // Alignment shifts glyph x within the line box, so it is applied before shaping to bake positions in.
    if let Some(a) = cosmic_align(style.text_align) {
        for line in buffer.lines.iter_mut() {
            line.set_align(Some(a));
        }
    }
    buffer.shape_until_scroll(font_system, false);
    buffer
}

/// `spans` cut to the first `len` bytes, for a paragraph the clamp has truncated. Without this a clamped mixed paragraph would carry ranges reaching past the text it now holds.
fn clip_spans(spans: Option<&[Span]>, len: usize) -> Option<Vec<Span>> {
    let spans = spans?;
    Some(
        spans
            .iter()
            .filter(|s| (s.range.start as usize) < len)
            .map(|s| Span {
                range: s.range.start..s.range.end.min(len as u32),
                over: s.over.clone(),
            })
            .collect(),
    )
}

/// cosmic-text reports glyph offsets within each logical line, so every visual line's start is shifted past the lines and line endings before it.
fn visual_line_starts(buffer: &Buffer) -> Vec<usize> {
    let line_offsets: Vec<usize> = buffer
        .lines
        .iter()
        .scan(0, |offset, line| {
            let start = *offset;
            *offset += line.text().len() + line.ending().as_str().len();
            Some(start)
        })
        .collect();
    buffer
        .layout_runs()
        .map(|run| {
            let within_line = run.glyphs.iter().map(|glyph| glyph.start).min();
            line_offsets[run.line_i] + within_line.unwrap_or(0)
        })
        .collect()
}

/// Shapes `text` into `rect`, then applies `max_lines`/`ellipsis` clamping: cosmic-text has no public ellipsis, so a clamped overflow is truncated at the start of the first dropped visual line, and (with ellipsis) `…` is appended and characters are dropped until it fits.
fn make_buffer(
    font_system: &mut FontSystem,
    family_availability: &mut rustc_hash::FxHashMap<Arc<str>, bool>,
    text: &str,
    spans: Option<&[Span]>,
    rect: Rect,
    style: &TextStyle,
) -> Buffer {
    let buffer = shape_buffer(font_system, family_availability, text, spans, rect, style);
    let Some(max) = style.clamp.max_lines() else {
        return buffer;
    };
    let line_starts = visual_line_starts(&buffer);
    if line_starts.len() <= max {
        return buffer;
    }
    let cut = line_starts[max].min(text.len());
    let head = text[..cut].trim_end();
    if !style.clamp.ellipsis() {
        let clipped = clip_spans(spans, head.len());
        return shape_buffer(
            font_system,
            family_availability,
            head,
            clipped.as_deref(),
            rect,
            style,
        );
    }
    // Appends `…`, dropping trailing chars until the result fits. The `…` takes the paragraph's own style, as CSS gives it the block's, and the spans are cut to what survives.
    let mut end = head.len();
    loop {
        let candidate = format!("{}\u{2026}", &head[..end]);
        let clipped = clip_spans(spans, end);
        let b = shape_buffer(
            font_system,
            family_availability,
            &candidate,
            clipped.as_deref(),
            rect,
            style,
        );
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

/// The cosmic-text slant for a style's [`FontStyle`]. `Oblique` was modelled by the shaper all along and unreachable from the vocabulary until it stopped being a boolean.
fn cosmic_style(font_style: FontStyle) -> Style {
    match font_style {
        FontStyle::Normal => Style::Normal,
        FontStyle::Italic => Style::Italic,
        FontStyle::Oblique => Style::Oblique,
    }
}

/// The cosmic-text family a style asks for, walking a [`FontFamily::Stack`] to the first member the font database actually has a face for — matching what the browser does with a CSS `font-family` list, rather than shaping every fallback chain in its first preference regardless of whether that face exists.
///
/// `SansSerif` is the database's own routed default, which [`Fonts::font_system`] has already pointed at the configured family — so a style naming nothing shapes exactly as it did before there was an axis to name, and a style naming a face reaches it without displacing anyone else's. `SystemUi` has no face of its own in fontdb's world (there is no platform UI catalogue to query, only a font database), so it resolves the same way: deterministic, if not distinct.
fn resolve_cosmic_family<'a>(
    family: &'a FontFamily,
    font_system: &mut FontSystem,
    family_availability: &mut rustc_hash::FxHashMap<Arc<str>, bool>,
) -> Family<'a> {
    match family {
        FontFamily::Stack(members) => members
            .iter()
            .find(|member| family_exists(member, font_system, family_availability))
            // Every member missing is a database with no fallback face at all — the same wall a single `Named` with no face has always hit, so it shapes in whatever `SansSerif` routes to rather than a face nobody has.
            .map_or(Family::SansSerif, |member| {
                resolve_cosmic_family(member, font_system, family_availability)
            }),
        other => bare_cosmic_family(other),
    }
}

/// One member of a fallback list, or the sole family when there is no list at all — never itself a `Stack`, since [`resolve_cosmic_family`] only ever calls this on a leaf.
fn bare_cosmic_family(family: &FontFamily) -> Family<'_> {
    match family {
        FontFamily::SansSerif | FontFamily::SystemUi => Family::SansSerif,
        FontFamily::Serif => Family::Serif,
        FontFamily::Monospace => Family::Monospace,
        FontFamily::Cursive => Family::Cursive,
        FontFamily::Fantasy => Family::Fantasy,
        FontFamily::Named(name) => Family::Name(name),
        // Unreachable through `resolve_cosmic_family`, which only ever hands this a leaf — kept as the total match's safety net rather than an `unreachable!()`, so a future caller that skips the stack walk still gets a deterministic answer instead of a panic.
        FontFamily::Stack(_) => Family::SansSerif,
    }
}

/// Whether `family` names a face the font database this shaper loaded actually has — a generic always does (it is the query's own vocabulary, not a name that can miss), a `Named` is asked of `fontdb` and the answer cached by name so a fallback chain shaped a thousand times queries the database once per distinct name in it, and a nested `Stack` exists if any of its own members do.
fn family_exists(
    family: &FontFamily,
    font_system: &mut FontSystem,
    family_availability: &mut rustc_hash::FxHashMap<Arc<str>, bool>,
) -> bool {
    match family {
        FontFamily::Named(name) => {
            if let Some(&known) = family_availability.get(name) {
                return known;
            }
            let found = font_system
                .db_mut()
                .query(&fontdb::Query {
                    families: &[fontdb::Family::Name(name)],
                    ..fontdb::Query::default()
                })
                .is_some();
            family_availability.insert(name.clone(), found);
            found
        }
        FontFamily::Stack(members) => members
            .iter()
            .any(|member| family_exists(member, font_system, family_availability)),
        _ => true,
    }
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
        let TextShaperConfig {
            raster,
            shaping,
            font,
        } = config;
        Self::in_fonts(&fonts::install(font), raster, shaping)
    }

    /// A shaper over faces already installed, for the measuring shaper: it shapes in whatever the process shapes in, so it has no configuration of its own to offer beyond the cache budgets.
    pub(crate) fn with_fonts(fonts: &Fonts) -> Self {
        let defaults = TextShaperConfig::default();
        Self::in_fonts(fonts, defaults.raster, defaults.shaping)
    }

    fn in_fonts(fonts: &Fonts, raster: Policy, shaping: Policy) -> Self {
        Self {
            font_system: fonts.font_system(),
            swash_cache: SwashCache::new(),
            atlas: GlyphAtlas::new(),
            raster_cache: Cache::new(raster, |pixels| pixels.len()),
            shaping_cache: Cache::new(shaping, |positions| {
                positions
                    .len()
                    .saturating_mul(size_of::<(CacheKey, i32, i32)>())
            }),
            measure_cache: Cache::new(limits::TEXT_MEASURE, |_| limits::SMALL_ENTRY_BYTES),
            has_colr_cache: Cache::new(limits::TEXT_HAS_COLR, |_| limits::SMALL_ENTRY_BYTES),
            colr_font_cache: Cache::new(limits::FONT_FILE, |font| font.0.len()),
            blank_glyphs: FxHashSet::default(),
            font_metrics_cache: None,
            family_availability: rustc_hash::FxHashMap::default(),
        }
    }

    /// Drops every raster, shaped run and measurement nothing has asked for within its idle horizon, and the glyph masks if they have outgrown their ceiling.
    ///
    /// The caches sweep themselves as they are used, which covers a running shell but not one that has stopped drawing — with no frames there are no accesses, and no access is a chance to reclaim. Call this when the app knows it has gone quiet.
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
    /// Counted from the caches themselves on each call, never tracked alongside them, so no number here can drift from what is true. The atlas and `swash_cache` are measured by hand — neither is a [`Cache`] — and the atlas reads zero on a software-only process, which is the point of it being lazy.
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
                // What the glyphs have written, not the plane they were written into: the plane is `mmap`'d zero pages that cost nothing until touched, and counting it made the census claim more memory than the process had.
                bytes: self.atlas.packed_bytes(),
                entries: self.atlas.glyph_count(),
                capacity: self.atlas.reserved_bytes(),
            },
        ]
    }

    /// Empties cosmic-text's glyph caches once the rasterized masks pass [`limits::GLYPH_RASTER_BUDGET_BYTES`].
    ///
    /// `SwashCache` is two `HashMap`s cosmic-text never evicts from. The hardware backend prunes it in passing — packing a glyph into a full atlas removes the evicted key — but the software backend, which reaches it through `Buffer::draw`, has nothing that ever removes anything. Left alone it grows with every distinct (glyph, size, subpixel bin) a session ever draws.
    ///
    /// All or nothing, because cosmic-text records no recency: with no way to tell a hot glyph from a cold one, evicting a chosen few would be guessing. Clearing costs re-rasterizing the glyphs still in use, which is why the ceiling sits far above what a shell actually holds and why this only ever runs from an idle sweep.
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

impl TextShaper {
    /// Whether `family` resolves to an installed face, asked of the database this shaper already loaded.
    ///
    /// The same query the sans-serif routing above makes. Exposed because the alternative an application reaches for is a second `fontdb::Database::load_system_fonts()`, which is a full font scan to answer a question this one can answer for free — and a second database that can disagree with the one the text is actually shaped in.
    pub fn family_available(&mut self, family: &str) -> bool {
        self.font_system
            .db_mut()
            .query(&fontdb::Query {
                families: &[fontdb::Family::Name(family)],
                ..fontdb::Query::default()
            })
            .is_some()
    }
}
