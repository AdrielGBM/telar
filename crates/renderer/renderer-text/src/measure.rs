use std::cell::RefCell;

use renderer_core::{FontConfig, TextRun, TextStyle};

use crate::{TextShaper, TextShaperConfig};

thread_local! {
    // A layout-thread-local shaper used only to size text during layout. Separate from the renderer's shaper (which may live on a render thread); created lazily on first use because building it loads fonts.
    static MEASURE_SHAPER: RefCell<Option<TextShaper>> = const { RefCell::new(None) };
    // Fonts the measure shaper should load, set by the runtime to match the renderer.
    static MEASURE_FONT_CONFIG: RefCell<Option<FontConfig>> = const { RefCell::new(None) };
}

/// Sets the fonts used by the layout-time text measurer to match the renderer.
/// Call once at startup (on the UI/layout thread): without the right fonts the
/// measurer falls back to system defaults, which on Android find no fonts and
/// abort cosmic-text with "no default font found".
pub fn set_measure_font_config(font: FontConfig) {
    MEASURE_FONT_CONFIG.with(|c| *c.borrow_mut() = Some(font));
    // Drop any shaper built with the previous fonts so the next measure rebuilds it.
    MEASURE_SHAPER.with(|s| *s.borrow_mut() = None);
    // Configuring the fonts a raster surface measures in says which measurer you want, so install it here rather than leave every runtime a second call to remember. It yields to a frontend that installed its own.
    renderer_core::set_default_text_metrics(ShaperMetrics);
}

/// [`renderer_core::TextMetrics`] over the layout-thread shaper: the answer for any surface whose text is glyphs.
pub struct ShaperMetrics;

impl renderer_core::TextMetrics for ShaperMetrics {
    fn measure(&self, text: &str, max_width: f32, style: &TextStyle) -> (f32, f32) {
        measure_text(text, max_width, style)
    }

    fn measure_runs(&self, runs: &[TextRun], max_width: f32, base: &TextStyle) -> (f32, f32) {
        measure_rich_text(runs, max_width, base)
    }

    fn ink_bounds(&self, text: &str, max_width: f32, style: &TextStyle) -> (f32, f32) {
        measure_ink_bounds(text, max_width, style)
    }

    fn line_height(&self, font_size: f32) -> f32 {
        font_size * crate::LINE_HEIGHT_FACTOR
    }
}

/// Measures the logical (width, height) of `text` wrapped to `max_width` for `style` — weight/italic
/// change glyph advances and `max_lines`/`ellipsis` clamp the extent, so measure and draw must agree by
/// using the same style. Intended for layout-time text sizing on the UI thread so a text node reserves
/// the height its lines actually need.
pub fn measure_text(text: &str, max_width: f32, style: &TextStyle) -> (f32, f32) {
    MEASURE_SHAPER.with(|s| {
        let mut slot = s.borrow_mut();
        let shaper = slot.get_or_insert_with(build_measure_shaper);
        shaper.measure_text(text, max_width, style)
    })
}

/// Measures a rich paragraph (styled runs) wrapped to `max_width` for layout-time sizing, so a rich-text node
/// reserves the height its runs need. The rich counterpart of [`measure_text`].
pub fn measure_rich_text(runs: &[TextRun], max_width: f32, base: &TextStyle) -> (f32, f32) {
    MEASURE_SHAPER.with(|s| {
        let mut slot = s.borrow_mut();
        let shaper = slot.get_or_insert_with(build_measure_shaper);
        shaper.measure_rich_text(runs, max_width, base)
    })
}

/// The text's ink bounding box `(ink_top, ink_height)` from the top of its layout rect — the actual drawn
/// glyph extent, not the full line box (see [`TextShaper::measure_ink_bounds`]). Lets a widget optically
/// center text vertically so a short run doesn't sit high next to an icon.
pub fn measure_ink_bounds(text: &str, max_width: f32, style: &TextStyle) -> (f32, f32) {
    MEASURE_SHAPER.with(|s| {
        let mut slot = s.borrow_mut();
        let shaper = slot.get_or_insert_with(build_measure_shaper);
        shaper.measure_ink_bounds(text, max_width, style)
    })
}

fn build_measure_shaper() -> TextShaper {
    let font = MEASURE_FONT_CONFIG
        .with(|c| c.borrow().clone())
        .or_else(android_fallback_fonts);
    match font {
        Some(font) => TextShaper::with_config(TextShaperConfig {
            font,
            ..Default::default()
        }),
        None => TextShaper::new(),
    }
}

// Safety net: if the runtime never set a config (e.g. a preview/test path), still point at the platform fonts on Android so the measurer doesn't abort.
fn android_fallback_fonts() -> Option<FontConfig> {
    cfg!(target_os = "android").then(|| FontConfig {
        system_fonts_dir: Some(std::path::PathBuf::from("/system/fonts")),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::measure_text;

    // Constraining a box to its own measured width must not push the text onto an extra line — i.e. the measured width must cover the line's full advance.
    #[test]
    fn measured_width_does_not_rewrap() {
        let style = renderer_core::TextStyle::new(14.0, renderer_core::Color::BLACK);
        let (w, h_unbounded) = measure_text("Features", 1.0e6, &style);
        let (_, h_at_measured) = measure_text("Features", w, &style);
        assert!(
            (h_unbounded - h_at_measured).abs() < 0.5,
            "box at measured width re-wrapped: w={w} h0={h_unbounded} h1={h_at_measured}"
        );
    }
}
