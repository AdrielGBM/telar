//! The layout-side measurer: how wide and how tall a string is, asked before anything is drawn.

use std::cell::RefCell;
use std::mem::ManuallyDrop;
use std::sync::Arc;

use renderer_core::{Span, TextStyle};

use crate::TextShaper;
use crate::fonts::{self, Fonts};

type Measurer = Option<(Arc<Fonts>, TextShaper)>;

reactive_local::dropless_thread_local! {
    // Used only to size text during layout, held with the faces it was built from. Separate from the renderer's shaper, which may live on a render thread, but never from its fonts. Created lazily, since building it clones a font database. Freed through `reactive_local::thread_exit`, so a hot-reloaded app holding one can still be unloaded.
    static MEASURE_SHAPER: ManuallyDrop<RefCell<Measurer>> = const { ManuallyDrop::new(RefCell::new(None)) };
}

/// Hands this thread's measuring shaper to `f`, rebuilding it first if the process now shapes in other faces.
///
/// The check is what holds "measure and draw agree" up: a shaper built from faces that are no longer installed is thrown away rather than trusted, so no ordering between building a renderer and laying out a tree can leave the measurer in the wrong fonts. It costs one lock read and a pointer compare per measurement.
fn with_shaper<R>(f: impl FnOnce(&mut TextShaper) -> R) -> R {
    let fonts = fonts::installed();
    MEASURE_SHAPER.with(|slot| {
        let mut slot = slot.borrow_mut();
        let current =
            matches!(slot.as_ref(), Some((built_from, _)) if Arc::ptr_eq(built_from, &fonts));
        if !current && slot.take().is_none() {
            reactive_local::thread_exit::on_thread_exit(release_shaper);
        }
        let (_, shaper) =
            slot.get_or_insert_with(|| (fonts.clone(), TextShaper::with_fonts(&fonts)));
        f(shaper)
    })
}

fn release_shaper() {
    MEASURE_SHAPER.with(|slot| slot.borrow_mut().take());
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

    fn min_content(&self, text: &str, spans: Option<&[Span]>, style: &TextStyle) -> (f32, f32) {
        measure_min_content(text, spans, style)
    }

    fn ink_bounds(&self, text: &str, max_width: f32, style: &TextStyle) -> (f32, f32) {
        measure_ink_bounds(text, max_width, style)
    }

    fn line_height(&self, font_size: f32) -> f32 {
        font_size * crate::LINE_HEIGHT_FACTOR
    }

    fn index_at(
        &self,
        text: &str,
        spans: Option<&[Span]>,
        max_width: f32,
        style: &TextStyle,
        at: (f32, f32),
    ) -> Option<usize> {
        with_shaper(|shaper| shaper.index_at(text, spans, max_width, style, at))
    }

    fn line_starts(
        &self,
        text: &str,
        spans: Option<&[Span]>,
        max_width: f32,
        style: &TextStyle,
    ) -> Vec<usize> {
        with_shaper(|shaper| shaper.line_starts(text, spans, max_width, style))
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

/// The narrowest `(width, height)` `text` lays out in without breaking inside a word — see [`TextShaper::measure_min_content`].
pub fn measure_min_content(text: &str, spans: Option<&[Span]>, style: &TextStyle) -> (f32, f32) {
    with_shaper(|shaper| shaper.measure_min_content(text, spans, style))
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
#[path = "measure_test.rs"]
mod tests;
