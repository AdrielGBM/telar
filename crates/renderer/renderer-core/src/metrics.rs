use std::sync::{Arc, RwLock};

use crate::{TextRun, TextStyle};

/// How much room a string takes — all a widget tree needs to know about text before anything is drawn.
///
/// A seam rather than a call into the shaper, because the answer belongs to the target: on a raster surface it is
/// cosmic-text's shaped advance, on a terminal it is `unicode-width` times a cell.
pub trait TextMetrics: Send + Sync + 'static {
    /// The logical `(width, height)` of `text` wrapped to `max_width` under `style`. Weight, slant, `max_lines`
    /// and `ellipsis` all change the extent, so measuring and drawing must be handed the same style.
    fn measure(&self, text: &str, max_width: f32, style: &TextStyle) -> (f32, f32);

    /// The same for a paragraph of styled runs, each measured against `base` wherever it overrides nothing.
    fn measure_runs(&self, runs: &[TextRun], max_width: f32, base: &TextStyle) -> (f32, f32);

    /// The drawn glyph extent `(ink_top, ink_height)` from the top of the layout rect, so a widget can optically
    /// centre a short run against something that is not text.
    fn ink_bounds(&self, text: &str, max_width: f32, style: &TextStyle) -> (f32, f32);

    /// The height of one line at `font_size`. A question rather than a constant because a terminal's line height
    /// is a cell, not a multiple of a font size.
    fn line_height(&self, font_size: f32) -> f32;
}

static TEXT_METRICS: RwLock<Option<Arc<dyn TextMetrics>>> = RwLock::new(None);

/// Installs the process-wide text measurer, replacing whatever was there.
pub fn set_text_metrics(metrics: impl TextMetrics) {
    *TEXT_METRICS.write().expect("text metrics lock") = Some(Arc::new(metrics));
}

/// Installs `metrics` only if nothing is installed yet, and reports whether it took.
///
/// What a runtime uses, so a frontend that already installed metrics of its own — cells for a terminal, a fixed
/// advance for a test — keeps them when the raster default is offered later.
pub fn set_default_text_metrics(metrics: impl TextMetrics) -> bool {
    let mut slot = TEXT_METRICS.write().expect("text metrics lock");
    if slot.is_some() {
        return false;
    }
    *slot = Some(Arc::new(metrics));
    true
}

fn metrics() -> Arc<dyn TextMetrics> {
    TEXT_METRICS
        .read()
        .expect("text metrics lock")
        .as_ref()
        .cloned()
        .expect(
            "no TextMetrics installed, so nothing can size text: install one with \
             renderer_core::set_text_metrics (renderer_text::ShaperMetrics is the raster default, and \
             building any renderer_text::TextShaper installs it for you)",
        )
}

/// Measures the logical `(width, height)` of `text` wrapped to `max_width`. See [`TextMetrics::measure`].
pub fn measure_text(text: &str, max_width: f32, style: &TextStyle) -> (f32, f32) {
    metrics().measure(text, max_width, style)
}

/// Measures a paragraph of styled runs. See [`TextMetrics::measure_runs`].
pub fn measure_rich_text(runs: &[TextRun], max_width: f32, base: &TextStyle) -> (f32, f32) {
    metrics().measure_runs(runs, max_width, base)
}

/// The text's drawn glyph extent `(ink_top, ink_height)`. See [`TextMetrics::ink_bounds`].
pub fn measure_ink_bounds(text: &str, max_width: f32, style: &TextStyle) -> (f32, f32) {
    metrics().ink_bounds(text, max_width, style)
}

/// The height of one line of text at `font_size`. See [`TextMetrics::line_height`].
pub fn line_height(font_size: f32) -> f32 {
    metrics().line_height(font_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed(f32);

    impl TextMetrics for Fixed {
        fn measure(&self, text: &str, _max_width: f32, _style: &TextStyle) -> (f32, f32) {
            (text.chars().count() as f32 * self.0, self.0)
        }
        fn measure_runs(
            &self,
            _runs: &[TextRun],
            _max_width: f32,
            _base: &TextStyle,
        ) -> (f32, f32) {
            (0.0, self.0)
        }
        fn ink_bounds(&self, _text: &str, _max_width: f32, _style: &TextStyle) -> (f32, f32) {
            (0.0, self.0)
        }
        fn line_height(&self, _font_size: f32) -> f32 {
            self.0
        }
    }

    // One test rather than three: the installed measurer is process-wide, so separate tests would race over it.
    #[test]
    fn a_frontends_own_metrics_survive_the_runtimes_default() {
        let style = TextStyle::new(14.0, crate::Color::BLACK);

        assert!(
            set_default_text_metrics(Fixed(10.0)),
            "the first default install takes"
        );
        assert_eq!(measure_text("ab", 1.0e6, &style).0, 20.0);

        assert!(
            !set_default_text_metrics(Fixed(1.0)),
            "a second default must not displace metrics already in force — that is the whole point of it"
        );
        assert_eq!(line_height(14.0), 10.0);

        set_text_metrics(Fixed(2.0));
        assert_eq!(
            line_height(14.0),
            2.0,
            "an explicit install replaces, so a frontend can change its mind"
        );
    }
}
