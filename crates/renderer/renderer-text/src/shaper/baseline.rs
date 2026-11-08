//! Where a line's baseline sits in its line box.
//!
//! cosmic-text centres the ascent and descent a face declares in the line box. A face that declares room above its capitals for stacked accents, as a platform's UI face does, declares more than a line holds, so centring hangs its descenders out of the bottom of every line and a box exactly one line tall clips them. What has to fit is the ink, so the line is moved by the ink of the letters that reach furthest, measured once per face and size so every string in a style keeps one baseline.

use geometry_core::Rect;
use renderer_core::TextStyle;

use super::cache::text_style_bits;
use super::{TextShaper, effective_line_height, physical_glyph, shape_buffer};

/// The Latin letters that reach furthest above and below the baseline: ascenders, which stand at or above the capitals, and descenders.
const REACH: &str = "bdfghjklpqy";
/// Room the reference cannot fill, finite because a shaper turns the width into whole pixels.
const REACH_WIDTH: f32 = 1_000.0;

impl TextShaper {
    /// How many whole pixels a line in `style` is drawn below where cosmic-text puts its baseline: the least move that keeps the ink of [`REACH`] inside the line box, or none when that ink is taller than the box. Whole pixels, so every glyph keeps the raster it had and only changes rows.
    pub(super) fn baseline_shift(&mut self, style: &TextStyle) -> i32 {
        let key = (style.font_size.to_bits(), text_style_bits(style));
        if let Some(shift) = self.baseline_cache.get(&key) {
            return *shift;
        }
        let shift = self.reach_ink(style).map_or(0, |(top, bottom)| {
            let highest = (-top).ceil();
            let lowest = (effective_line_height(style) - bottom).floor();
            if highest <= lowest {
                0.0_f32.clamp(highest, lowest) as i32
            } else {
                0
            }
        });
        self.baseline_cache.insert(key, shift);
        shift
    }

    /// The rows the ink of [`REACH`] spans in its first line, as cosmic-text places it, from the top of that line box.
    fn reach_ink(&mut self, style: &TextStyle) -> Option<(f32, f32)> {
        let rect = Rect {
            x: 0.0,
            y: 0.0,
            width: REACH_WIDTH,
            height: effective_line_height(style),
        };
        let buffer = shape_buffer(
            &mut self.font_system,
            &mut self.family_availability,
            REACH,
            None,
            rect,
            style,
        );
        let run = buffer.layout_runs().next()?;
        let baseline = run.line_y - run.line_top;
        let glyphs: Vec<_> = run
            .glyphs
            .iter()
            .map(|glyph| {
                let placed = physical_glyph(glyph, (0.0, baseline), 1.0, style.raster);
                (placed.cache_key, placed.y)
            })
            .collect();
        drop(buffer);
        self.ink_extent(glyphs)
    }
}

#[cfg(test)]
#[path = "baseline.test.rs"]
mod tests;
