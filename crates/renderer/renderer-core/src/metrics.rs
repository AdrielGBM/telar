//! The installed text measurer: how wide a string is, asked without naming a shaper.

use std::sync::{Arc, RwLock};

use crate::{Span, TextStyle};

/// How much room a string takes — all a widget tree needs to know about text before anything is drawn.
///
/// A seam rather than a call into the shaper, because the answer belongs to the target: on a raster surface it is cosmic-text's shaped advance, on a terminal it is `unicode-width` times a cell.
pub trait TextMetrics: Send + Sync + 'static {
    /// The logical `(width, height)` of `text` wrapped to `max_width` under `style`, with `spans` overriding it over their byte ranges. Weight, slant, `max_lines` and `ellipsis` all change the extent, so measuring and drawing must be handed the same style — and the same spans.
    fn measure(
        &self,
        text: &str,
        spans: Option<&[Span]>,
        max_width: f32,
        style: &TextStyle,
    ) -> (f32, f32);

    /// The drawn glyph extent `(ink_top, ink_height)` from the top of the layout rect, so a widget can optically centre a short run against something that is not text.
    fn ink_bounds(&self, text: &str, max_width: f32, style: &TextStyle) -> (f32, f32);

    /// The height of one line at `font_size`. A question rather than a constant because a terminal's line height is a cell, not a multiple of a font size.
    fn line_height(&self, font_size: f32) -> f32;
}

static TEXT_METRICS: RwLock<Option<Arc<dyn TextMetrics>>> = RwLock::new(None);

/// Installs the process-wide text measurer, replacing whatever was there.
pub fn set_text_metrics(metrics: impl TextMetrics) {
    *TEXT_METRICS.write().expect("text metrics lock") = Some(Arc::new(metrics));
}

/// Installs `metrics` only if nothing is installed yet, and reports whether it took.
///
/// What a runtime uses, so a frontend that already installed metrics of its own — cells for a terminal, a fixed advance for a test — keeps them when the raster default is offered later.
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
pub fn measure_text(
    text: &str,
    spans: Option<&[Span]>,
    max_width: f32,
    style: &TextStyle,
) -> (f32, f32) {
    metrics().measure(text, spans, max_width, style)
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
#[path = "metrics_test.rs"]
mod tests;
