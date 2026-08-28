//! Layout-time text measurement, as the widgets reach it.
//!
//! The indirection exists for the test-time install below: `renderer-text` is a dev-dependency now, so without it
//! every test that lays out a label would have to name a shaper itself.

use renderer_core::{Span, TextStyle};

// Idempotent and yielding, so a test that installed its own stub keeps it.
#[cfg(test)]
fn ensure_installed() {
    renderer_core::set_default_text_metrics(renderer_text::ShaperMetrics);
}

pub(crate) fn measure_text(
    text: &str,
    spans: Option<&[Span]>,
    max_width: f32,
    style: &TextStyle,
) -> (f32, f32) {
    #[cfg(test)]
    ensure_installed();
    renderer_core::measure_text(text, spans, max_width, style)
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

/// The height of one line **in this style**, which is the one the shaper will lay out: the multiple the style
/// declares, or the face's own when it declares none.
///
/// The size alone is not the question, and asking it that way is how a caret ends up hanging below the
/// letters it stands in. A tree that declares `line_height: 1.0` — a pixel face kept on its own grid — gets
/// lines the height of the em, while [`line_height`] goes on answering with the face's natural leading, and
/// every rectangle drawn to that answer is taller than the line it belongs to.
pub(crate) fn line_box(style: &TextStyle) -> f32 {
    match style.line_height.factor() {
        Some(factor) => style.font_size * factor,
        None => line_height(style.font_size),
    }
}
