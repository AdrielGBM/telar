//! Where a line of text sits in a box taller than it: the band from its baseline to its cap height in the middle of the box, the one rule a text leaf and an editor's line both follow.

use std::cell::RefCell;

use renderer_core::TextStyle;

/// The run whose ink is the band a text is centred on: a capital H, the letter a face's cap height is defined by, whose flat top and foot sit exactly on the cap height and the baseline, so the body of the letters is centred and ascenders and descenders hang outside the band; a round or pointed letter would overshoot both lines by an amount that differs from face to face. Measuring one fixed run rather than each string is what puts every label of a style on one baseline.
pub(crate) const REFERENCE: &str = "H";
/// Room the reference cannot fill, finite because a shaper turns the width into whole pixels.
const REFERENCE_WIDTH: f32 = 1_000.0;

/// The cap band of [`REFERENCE`] inside one line box, and that line box's height.
#[derive(Clone, Copy)]
struct CapBand {
    top: f32,
    height: f32,
    line: f32,
}

/// The cap band of the last face asked about, kept by whatever places text in it.
#[derive(Default)]
pub(crate) struct CapBandCache(RefCell<Option<(TextStyle, u64, CapBand)>>);

impl CapBandCache {
    /// The top of a block of text `block` tall, laid in lines of `style`, inside a box `height` tall: the block's whole line box drawn, with the cap band of its style in the middle of the box.
    pub(crate) fn top_in(&self, height: f32, block: f32, style: &TextStyle) -> f32 {
        let band = self.band(style);
        // The nudge belongs to the face, so it applies once to the centred block however many lines it has, which centres the span from the first line's cap height to the last line's baseline.
        let nudge = if band.height > 0.0 {
            band.line / 2.0 - band.top - band.height / 2.0
        } else {
            0.0
        };
        // Snapped vertically only, to the rows of the layout grid (pixels, or a terminal's cells), since a glyph drawn half a row down is resampled across two rows and goes soft while horizontal subpixel placement keeps letter spacing even; clamped so the nudge never moves the line box, descenders included, out of the box.
        let row = geometry_core::layout_grid().y;
        let slack = (height - block).max(0.0);
        let centred = (slack / 2.0 + nudge).clamp(0.0, slack) / row;
        // A half, give or take the noise of the sums before it, snaps toward the top, so an odd spare row goes below the band where descenders hang rather than above the capitals.
        let snapped = (centred - 0.5 - 1.0e-3).ceil();
        if snapped <= 0.0 {
            0.0
        } else {
            snapped.min((slack / row).floor()) * row
        }
    }

    fn band(&self, style: &TextStyle) -> CapBand {
        let face = face_of(style);
        let generation = renderer_core::text_metrics_generation();
        let mut cache = self.0.borrow_mut();
        if let Some((cached_face, cached_generation, band)) = cache.as_ref()
            && *cached_generation == generation
            && *cached_face == face
        {
            return *band;
        }
        let (top, height) =
            crate::text_metrics::measure_ink_bounds(REFERENCE, REFERENCE_WIDTH, &face);
        let (_, line) = crate::text_metrics::measure_text(REFERENCE, None, REFERENCE_WIDTH, &face);
        let band = CapBand { top, height, line };
        *cache = Some((face, generation, band));
        band
    }
}

/// What of `style` decides the reference's glyphs and line box; the case stays as written, so a lowercased style still measures a capital.
fn face_of(style: &TextStyle) -> TextStyle {
    TextStyle {
        font_family: style.font_family.clone(),
        font_weight: style.font_weight,
        font_style: style.font_style,
        line_height: style.line_height,
        raster: style.raster,
        font_variations: style.font_variations.clone(),
        font_features: style.font_features.clone(),
        lang: style.lang.clone(),
        ..TextStyle::new(style.font_size, renderer_core::Color::BLACK)
    }
}

#[cfg(test)]
#[path = "cap_band_test.rs"]
mod tests;
