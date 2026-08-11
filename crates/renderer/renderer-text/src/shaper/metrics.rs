use super::TextShaper;
use cosmic_text::fontdb;

impl TextShaper {
    /// Returns real ascender/line-height metrics for the default sans-serif face, expressed as
    /// ratios relative to `font_size`. Reads the font's metrics via skrifa once and caches the
    /// result; falls back to conservative defaults if the default font cannot be resolved.
    pub fn font_metrics(&mut self) -> renderer_core::FontMetrics {
        if let Some(cached) = self.font_metrics_cache {
            return cached;
        }
        let metrics = self
            .default_font_id()
            .and_then(|id| self.font_data_for(id))
            .and_then(|(bytes, index)| Self::metrics_from_font(&bytes, index))
            .unwrap_or_default();
        self.font_metrics_cache = Some(metrics);
        metrics
    }

    /// Resolves the font db id of the configured sans-serif family (the default for unstyled text).
    // pub(super): called from `shaper::colr`, a sibling module, via `default_face_id`.
    pub(super) fn default_font_id(&self) -> Option<fontdb::ID> {
        let db = self.font_system.db();
        db.query(&fontdb::Query {
            families: &[fontdb::Family::SansSerif],
            ..fontdb::Query::default()
        })
    }

    /// Reads ascent and full line height (ascent + descent + line gap) from a font via skrifa,
    /// converting them into ratios relative to `font_size` (em). Returns `None` if the font or its
    /// units-per-em are unusable.
    fn metrics_from_font(bytes: &[u8], index: u32) -> Option<renderer_core::FontMetrics> {
        use skrifa::{
            instance::{LocationRef, Size as SkrifaSize},
            metrics::Metrics as SkrifaMetrics,
        };
        let font_ref = skrifa::FontRef::from_index(bytes, index).ok()?;
        // Size::new(1.0) yields metrics already normalized to fractions of the em (font_size).
        let m = SkrifaMetrics::new(&font_ref, SkrifaSize::new(1.0), LocationRef::default());
        // descent is negative (below baseline); ascent + |descent| + leading is the full line box.
        let line_height_factor = (m.ascent - m.descent + m.leading).max(0.0);
        // ascender_ratio is the overshoot ABOVE the rect's top edge, not the full ascent. The em box top sits at 1.0 em above the baseline; any ascent beyond that is real overshoot. glyph bbox (when present) can reach higher than the hhea ascent, so prefer it.
        let glyph_top = m.bounds.map(|b| b.y_max).filter(|v| v.is_finite());
        let top = glyph_top.unwrap_or(m.ascent).max(m.ascent);
        let ascender_ratio = (top - 1.0).max(0.0);
        if !line_height_factor.is_finite()
            || line_height_factor <= 0.0
            || !ascender_ratio.is_finite()
        {
            return None;
        }
        Some(renderer_core::FontMetrics {
            line_height_factor,
            ascender_ratio,
        })
    }

    pub fn font_data_for(&self, font_id: fontdb::ID) -> Option<(Vec<u8>, u32)> {
        let (source, index) = self.font_system.db().face_source(font_id)?;
        let bytes = match source {
            fontdb::Source::Binary(arc) => arc.as_ref().as_ref().to_vec(),
            fontdb::Source::File(path) => std::fs::read(path).ok()?,
            fontdb::Source::SharedFile(path, _) => std::fs::read(path).ok()?,
        };
        Some((bytes, index))
    }
}
