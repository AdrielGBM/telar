//! The layout-side measurer: how wide and how tall a string is, asked before anything is drawn.

use std::cell::RefCell;
use std::sync::Arc;

use renderer_core::{Span, TextStyle};

use crate::TextShaper;
use crate::fonts::{self, Fonts};

thread_local! {
    // Used only to size text during layout, held with the faces it was built from. Separate from the renderer's shaper, which may live on a render thread, but never from its fonts. Created lazily, since building it clones a font database.
    static MEASURE_SHAPER: RefCell<Option<(Arc<Fonts>, TextShaper)>> = const { RefCell::new(None) };
}

/// Hands this thread's measuring shaper to `f`, rebuilding it first if the process now shapes in other faces.
///
/// The check is what holds "measure and draw agree" up: a shaper built from faces that are no longer installed is thrown away rather than trusted, so no ordering between building a renderer and laying out a tree can leave the measurer in the wrong fonts. It costs one lock read and a pointer compare per measurement.
fn with_shaper<R>(f: impl FnOnce(&mut TextShaper) -> R) -> R {
    let fonts = fonts::installed();
    MEASURE_SHAPER.with(|slot| {
        let mut slot = slot.borrow_mut();
        if matches!(slot.as_ref(), Some((built_from, _)) if !Arc::ptr_eq(built_from, &fonts)) {
            *slot = None;
        }
        let (_, shaper) =
            slot.get_or_insert_with(|| (fonts.clone(), TextShaper::with_fonts(&fonts)));
        f(shaper)
    })
}

/// [`renderer_core::TextMetrics`] over the layout-thread shaper: the answer for any surface whose text is glyphs.
pub struct ShaperMetrics;

impl renderer_core::TextMetrics for ShaperMetrics {
    fn measure(
        &self,
        text: &str,
        spans: Option<&[Span]>,
        max_width: f32,
        style: &TextStyle,
    ) -> (f32, f32) {
        measure_text(text, spans, max_width, style)
    }

    fn ink_bounds(&self, text: &str, max_width: f32, style: &TextStyle) -> (f32, f32) {
        measure_ink_bounds(text, max_width, style)
    }

    fn line_height(&self, font_size: f32) -> f32 {
        font_size * crate::LINE_HEIGHT_FACTOR
    }
}

/// Measures the logical (width, height) of `text` wrapped to `max_width` for `style` — weight/italic change glyph advances and `max_lines`/`ellipsis` clamp the extent, so measure and draw must agree by using the same style. Intended for layout-time text sizing on the UI thread so a text node reserves the height its lines actually need.
pub fn measure_text(
    text: &str,
    spans: Option<&[Span]>,
    max_width: f32,
    style: &TextStyle,
) -> (f32, f32) {
    with_shaper(|shaper| shaper.measure_text(text, spans, max_width, style))
}

/// The text's ink bounding box `(ink_top, ink_height)` from the top of its layout rect — the actual drawn glyph extent, not the full line box (see [`TextShaper::measure_ink_bounds`]). Lets a widget optically center text vertically so a short run doesn't sit high next to an icon.
pub fn measure_ink_bounds(text: &str, max_width: f32, style: &TextStyle) -> (f32, f32) {
    with_shaper(|shaper| shaper.measure_ink_bounds(text, max_width, style))
}

/// Whether `family` names a font installed on this system, answered by the database the layout-time shaper already loaded — no second scan, and no second database to disagree with the one text is shaped in.
///
/// Answers against the faces [`fonts::install`] last made current, so a family bundled with the application counts as available exactly when the shaper can use it.
pub fn font_family_available(family: &str) -> bool {
    with_shaper(|shaper| shaper.family_available(family))
}

#[cfg(test)]
mod tests {
    use super::measure_text;

    // Constraining a box to its own measured width must not push the text onto an extra line, so the measured width must cover the line's full advance.
    #[test]
    fn measured_width_does_not_rewrap() {
        let style = renderer_core::TextStyle::new(14.0, renderer_core::Color::BLACK);
        let (w, h_unbounded) = measure_text("Features", None, 1.0e6, &style);
        let (_, h_at_measured) = measure_text("Features", None, w, &style);
        assert!(
            (h_unbounded - h_at_measured).abs() < 0.5,
            "box at measured width re-wrapped: w={w} h0={h_unbounded} h1={h_at_measured}"
        );
    }
}
