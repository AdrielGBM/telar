//! Layout-time text measurement, as the widgets reach it.
//!
//! The indirection exists for the test-time install below: `renderer-text` is a dev-dependency now, so without it
//! every test that lays out a label would have to name a shaper itself.

use renderer_core::{TextRun, TextStyle};

// Idempotent and yielding, so a test that installed its own stub keeps it.
#[cfg(test)]
fn ensure_installed() {
    renderer_core::set_default_text_metrics(renderer_text::ShaperMetrics);
}

pub(crate) fn measure_text(text: &str, max_width: f32, style: &TextStyle) -> (f32, f32) {
    #[cfg(test)]
    ensure_installed();
    renderer_core::measure_text(text, max_width, style)
}

pub(crate) fn measure_rich_text(runs: &[TextRun], max_width: f32, base: &TextStyle) -> (f32, f32) {
    #[cfg(test)]
    ensure_installed();
    renderer_core::measure_rich_text(runs, max_width, base)
}

pub(crate) fn measure_ink_bounds(text: &str, max_width: f32, style: &TextStyle) -> (f32, f32) {
    #[cfg(test)]
    ensure_installed();
    renderer_core::measure_ink_bounds(text, max_width, style)
}

pub(crate) fn line_height(font_size: f32) -> f32 {
    #[cfg(test)]
    ensure_installed();
    renderer_core::line_height(font_size)
}
