use skrifa::MetadataProvider;

use super::*;

const COLOR_FACE: &[u8] = include_bytes!("../test-fonts/TelarTestColor.ttf");
const SIZE: f32 = 100.0;
const TEXT: [u8; 4] = [0, 0, 0, 255];
const PAINTED_RED_AT_HALF_ALPHA: [u8; 4] = [255, 0, 0, 128];

fn glyph_of(character: char) -> u16 {
    let font = FontRef::new(COLOR_FACE).unwrap();
    font.charmap().map(character).unwrap().to_u32() as u16
}

fn letter_l() -> ColrGlyphBitmap {
    rasterize_colr_glyph(COLOR_FACE, 0, glyph_of('L'), SIZE, TEXT).unwrap()
}

fn pixel(bitmap: &ColrGlyphBitmap, x: u32, y: u32) -> [u8; 4] {
    let at = ((y * bitmap.width + x) * 4) as usize;
    bitmap.rgba[at..at + 4].try_into().unwrap()
}

fn inked_rows(bitmap: &ColrGlyphBitmap) -> Vec<u32> {
    (0..bitmap.height)
        .filter(|&y| (0..bitmap.width).any(|x| pixel(bitmap, x, y)[3] != 0))
        .collect()
}

#[test]
fn the_bitmap_covers_the_font_bounds_with_an_antialiasing_margin() {
    let bitmap = letter_l();

    assert_eq!((bitmap.width, bitmap.height), (125, 125));
    assert_eq!(bitmap.rgba.len(), 125 * 125 * 4);
}

#[test]
fn the_glyph_is_upright_with_its_foot_pointing_right() {
    let bitmap = letter_l();
    let top_of_stem = 30;
    let foot = bitmap.placement_top as u32 - 4;

    assert_eq!(pixel(&bitmap, 12, top_of_stem), PAINTED_RED_AT_HALF_ALPHA);
    assert_eq!(pixel(&bitmap, 40, foot), PAINTED_RED_AT_HALF_ALPHA);
    assert_eq!(pixel(&bitmap, 40, top_of_stem)[3], 0);
    assert_eq!(pixel(&bitmap, 40, 20)[3], 0);
}

#[test]
fn pixels_are_straight_alpha_rather_than_premultiplied() {
    let bitmap = letter_l();

    let [red, green, blue, alpha] = pixel(&bitmap, 12, 60);

    assert_eq!((red, green, blue), (255, 0, 0));
    assert!(
        alpha.abs_diff(128) <= 1,
        "half-transparent paint must read back as about half alpha, got {alpha}"
    );
}

#[test]
fn the_baseline_sits_placement_top_rows_down_with_no_horizontal_offset() {
    let bitmap = letter_l();
    let rows = inked_rows(&bitmap);

    assert_eq!(bitmap.placement_left, 0);
    assert_eq!(*rows.last().unwrap() + 1, bitmap.placement_top as u32);
    assert!(
        *rows.first().unwrap() < bitmap.placement_top as u32 - 60,
        "the letter must rise well above the baseline"
    );
}

#[test]
fn blitting_at_the_pen_lands_the_baseline_on_the_pen() {
    let bitmap = letter_l();
    let pen_y = 200;

    let blit_top = pen_y - bitmap.placement_top;
    let foot_bottom = blit_top + *inked_rows(&bitmap).last().unwrap() as i32 + 1;

    assert_eq!(foot_bottom, pen_y);
}

#[test]
fn a_glyph_without_a_colour_record_is_not_rasterized() {
    assert!(rasterize_colr_glyph(COLOR_FACE, 0, glyph_of('A'), SIZE, TEXT).is_none());
}

#[test]
fn a_size_that_is_not_positive_is_not_rasterized() {
    assert!(rasterize_colr_glyph(COLOR_FACE, 0, glyph_of('L'), 0.0, TEXT).is_none());
    assert!(rasterize_colr_glyph(COLOR_FACE, 0, glyph_of('L'), -4.0, TEXT).is_none());
}

#[test]
fn bytes_that_are_not_a_font_are_not_rasterized() {
    assert!(rasterize_colr_glyph(b"not a font", 0, 1, SIZE, TEXT).is_none());
}
