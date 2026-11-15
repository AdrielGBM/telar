//! The text measurer a surface lays out with: how wide a string is, asked without naming a shaper.

use crate::{Span, TextLine, TextStyle};

/// How much room a string takes — all a widget tree needs to know about text before anything is drawn.
///
/// A seam rather than a call into the shaper, because the answer belongs to the target: on a raster surface it is cosmic-text's shaped advance, on a terminal it is `unicode-width` times a cell. The renderer that draws a surface says which one it measures with ([`RendererFactory::text_metrics`](crate::RendererFactory::text_metrics)), so two surfaces drawn by two renderers each lay their text out the way it will be drawn.
pub trait TextMetrics: Send + Sync + 'static {
    /// The logical `(width, height)` of `text` wrapped to `max_width` under `style`, with `spans` overriding it over their byte ranges. Weight, slant, `max_lines` and `ellipsis` all change the extent, so measuring and drawing must be handed the same style — and the same spans.
    fn measure(
        &self,
        text: &str,
        spans: Option<&[Span]>,
        max_width: f32,
        style: &TextStyle,
    ) -> (f32, f32);

    /// The narrowest `(width, height)` `text` lays out in without breaking inside a word — its widest unbreakable run, and the height it wraps to at that width, as CSS's min-content size — which [`measure`](Self::measure) at a width of zero is not: that breaks a word wherever it must, a last resort for drawing into a box already too narrow rather than a reason to make the box narrow.
    fn min_content(&self, text: &str, spans: Option<&[Span]>, style: &TextStyle) -> (f32, f32);

    /// The drawn glyph extent `(ink_top, ink_height)` from the top of the layout rect, so a widget can optically centre a short run against something that is not text.
    fn ink_bounds(&self, text: &str, max_width: f32, style: &TextStyle) -> (f32, f32);

    /// The height of one line at `font_size`. A question rather than a constant because a terminal's line height is a cell, not a multiple of a font size.
    fn line_height(&self, font_size: f32) -> f32;

    /// How many times this measurer may have started answering differently for the same text and style: a face arrived, so a family that was falling back resolves to a face of its own. Layout compares it against the one it last measured at and measures every text again when the two differ, so a measurer whose answers never change keeps the default.
    fn generation(&self) -> u64 {
        0
    }

    /// The lines of `text` laid out at `max_width` as they are drawn, with the same breaks, alignment and reordering, each cut into the grapheme clusters a caret steps over, in bytes of `text` as written. What an editor breaks its rows at and places its caret, its selection and a press by, so a surface that reorders right-to-left runs answers with them reordered, and one that does not answers in the order the text is written.
    fn laid_lines(
        &self,
        text: &str,
        spans: Option<&[Span]>,
        max_width: f32,
        style: &TextStyle,
    ) -> Vec<TextLine>;

    /// The byte offset of the character under `(x, y)` in `text` laid out at `max_width` the way [`measure`](Self::measure) lays it out, from the top-left of the text. `None` off the text, and on a surface that cannot tell: a document answers a press on a run itself.
    fn index_at(
        &self,
        _text: &str,
        _spans: Option<&[Span]>,
        _max_width: f32,
        _style: &TextStyle,
        _at: (f32, f32),
    ) -> Option<usize> {
        None
    }

    /// The font size at which `text`, on one line, is `width` wide and no taller than `max_height`, with `style_at` giving the text's style at a size. `None` where the line's extent does not grow with its size: an empty string, or a terminal's cells.
    ///
    /// Measured at two sizes and solved rather than measured at one and scaled, because a line's width is linear in its size plus whatever does not scale with it: tracking or a span declared in pixels.
    fn fitted_size(
        &self,
        text: &str,
        spans: Option<&[Span]>,
        width: f32,
        max_height: Option<f32>,
        style_at: &dyn Fn(f32) -> TextStyle,
    ) -> Option<f32> {
        let line_at = |size: f32| {
            let style = style_at(size).with_text_wrap(crate::TextWrap::NoWrap);
            self.measure(text, spans, FIT_UNBOUNDED_WIDTH, &style)
        };
        let (near_width, near_height) = line_at(FIT_PROBE_SIZE);
        let (far_width, far_height) = line_at(2.0 * FIT_PROBE_SIZE);
        let size_at = |target: f32, near: f32, far: f32| {
            let slope = (far - near) / FIT_PROBE_SIZE;
            (slope.is_finite() && slope > f32::EPSILON)
                .then(|| FIT_PROBE_SIZE + (target - near) / slope)
        };
        let by_width = size_at(width, near_width, far_width)?;
        let by_height = max_height.and_then(|height| size_at(height, near_height, far_height));
        Some(by_height.map_or(by_width, |cap| by_width.min(cap)).max(0.0))
    }
}

const FIT_PROBE_SIZE: f32 = 100.0;
const FIT_UNBOUNDED_WIDTH: f32 = 1.0e6;

#[cfg(test)]
#[path = "metrics.test.rs"]
mod tests;
