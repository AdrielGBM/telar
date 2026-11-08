//! Guards over what the shaping, raster and COLR cache keys distinguish.

use super::*;
use crate::shaper::TextShaper;
use geometry_core::Rect;
use renderer_core::{Color, FontStyle, TextAlign};

const STYLES: [FontStyle; 3] = [FontStyle::Normal, FontStyle::Italic, FontStyle::Oblique];
const ALIGNS: [TextAlign; 4] = [
    TextAlign::Start,
    TextAlign::Center,
    TextAlign::End,
    TextAlign::Justify,
];

fn styled(font_style: FontStyle, text_align: TextAlign) -> TextStyle {
    let mut style = TextStyle::new(16.0, Color::BLACK);
    style.font_style = font_style;
    style.text_align = text_align;
    style
}

#[test]
fn every_font_style_and_alignment_pair_has_its_own_bits() {
    let mut seen = std::collections::HashMap::new();
    for font_style in STYLES {
        for align in ALIGNS {
            let bits = text_style_bits(&styled(font_style, align));
            let clash = seen.insert(bits, (font_style, align));
            assert_eq!(clash, None, "{font_style:?}/{align:?} shares its bits");
        }
    }
}

#[test]
fn a_normal_start_aligned_style_keeps_the_default_key() {
    assert_eq!(
        text_style_bits(&styled(FontStyle::Normal, TextAlign::Start)),
        400
    );
}

#[test]
fn a_height_grown_after_first_shaping_shows_every_line() {
    let mut shaper = TextShaper::new();
    let text = "one two three four five six seven eight nine ten eleven twelve ".repeat(4);
    let style = TextStyle::new(16.0, Color::BLACK);
    let rect = |height| Rect {
        x: 0.0,
        y: 0.0,
        width: 200.0,
        height,
    };
    let (mut short, mut tall, mut fresh) = (Vec::new(), Vec::new(), Vec::new());
    shaper.layout_glyphs(&text, None, rect(10.0), &style, 1.0, &mut short);
    shaper.layout_glyphs(&text, None, rect(400.0), &style, 1.0, &mut tall);
    TextShaper::new().layout_glyphs(&text, None, rect(400.0), &style, 1.0, &mut fresh);
    assert!(short.len() < tall.len());
    assert_eq!(tall.len(), fresh.len());
}

#[test]
fn a_fractional_width_is_a_different_key_from_its_ceiling() {
    let mut shaper = TextShaper::new();
    let text = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda";
    let style = TextStyle::new(16.0, Color::BLACK);
    let rect = |width| Rect {
        x: 0.0,
        y: 0.0,
        width,
        height: 400.0,
    };
    let (mut narrow, mut whole, mut fresh) = (Vec::new(), Vec::new(), Vec::new());
    shaper.layout_glyphs(text, None, rect(119.2), &style, 1.0, &mut narrow);
    shaper.layout_glyphs(text, None, rect(120.0), &style, 1.0, &mut whole);
    TextShaper::new().layout_glyphs(text, None, rect(120.0), &style, 1.0, &mut fresh);
    let xs = |glyphs: &[crate::GlyphInfo]| {
        glyphs
            .iter()
            .map(|g| g.dest_rect.map(f32::to_bits))
            .collect::<Vec<_>>()
    };
    assert_eq!(xs(&whole), xs(&fresh));
}
