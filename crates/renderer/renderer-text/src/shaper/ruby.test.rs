//! Readings over their bases: the room a line makes for them, the base that never breaks, the reading that fits before it widens, and a press that still finds its run.

use crate::TextShaper;
use geometry_core::{Color, Rect};
use renderer_core::{Declared, FontFamily, Span, TextStyle, TextWrap};

const TEST_FACE: &[u8] = include_bytes!("../../test-fonts/TelarTest.ttf");

fn style() -> TextStyle {
    let _installing = crate::fonts::FACES_STABLE
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let families = crate::fonts::install_face(TEST_FACE.to_vec())
        .expect("the bundled face must load, or text is drawn in whatever the machine has");
    TextStyle::new(20.0, Color::BLACK)
        .with_font_family(FontFamily::Named(families[0].as_str().into()))
}

fn reading(range: std::ops::Range<u32>, reading: &str) -> Span {
    Span::new(range, Declared::default()).with_ruby(reading)
}

fn plain(range: std::ops::Range<u32>) -> Span {
    Span::new(range, Declared::default())
}

#[test]
fn a_line_with_a_reading_grows_to_hold_it() {
    let mut sh = TextShaper::new();
    let style = style();
    let (plain_width, plain_height) =
        sh.measure_text("base text", Some(&[plain(0..4)]), 400.0, &style);
    let (width, height) = sh.measure_text("base text", Some(&[reading(0..4, "bs")]), 400.0, &style);
    assert!(height > plain_height, "{height} over {plain_height}");
    assert_eq!(
        width, plain_width,
        "a reading narrower than its base leaves the line as wide"
    );
}

#[test]
fn only_the_lines_holding_a_reading_grow() {
    let mut sh = TextShaper::new();
    let style = style();
    let text = "first line words\nsecond";
    let (_, plain) = sh.measure_text(text, Some(&[plain(0..5)]), 400.0, &style);
    let (_, first) = sh.measure_text(text, Some(&[reading(0..5, "a")]), 400.0, &style);
    let (_, both) = sh.measure_text(
        text,
        Some(&[reading(0..5, "a"), reading(17..23, "b")]),
        400.0,
        &style,
    );
    assert!(first > plain);
    assert!(both > first, "the second line grows for its own reading");
    assert!(((both - plain) - 2.0 * (first - plain)).abs() < 1e-3);
}

#[test]
fn a_line_whose_leading_already_holds_the_reading_keeps_its_height() {
    let mut sh = TextShaper::new();
    let style = style().with_line_height(2.6);
    let (_, plain) = sh.measure_text("base text", Some(&[plain(0..4)]), 400.0, &style);
    let (_, height) = sh.measure_text("base text", Some(&[reading(0..4, "bs")]), 400.0, &style);
    assert_eq!(height, plain);
}

#[test]
fn a_base_is_never_broken_across_lines() {
    let mut sh = TextShaper::new();
    let style = style();
    let text = "wwww one two";
    let base = 5..12;
    let (narrow, _) = sh.measure_text("wwww one", None, 400.0, &style);
    let holds_base = |lines: &[renderer_core::TextLine]| {
        lines
            .iter()
            .any(|line| line.range.start <= base.start && line.range.end >= base.end)
    };
    let unglued = sh.laid_lines(text, Some(&[plain(5..12)]), narrow, &style);
    assert!(
        !holds_base(&unglued),
        "without a reading the words wrap apart"
    );
    let glued = sh.laid_lines(text, Some(&[reading(5..12, "r")]), narrow, &style);
    assert!(
        holds_base(&glued),
        "the base moves to the next line whole: {:?}",
        glued
            .iter()
            .map(|line| line.range.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_reading_a_little_longer_than_its_base_shrinks_instead_of_widening_it() {
    let mut sh = TextShaper::new();
    let style = style().with_text_wrap(TextWrap::NoWrap);
    let (plain_width, _) = sh.measure_text("mm x", Some(&[plain(0..2)]), 400.0, &style);
    let (width, _) = sh.measure_text("mm x", Some(&[reading(0..2, "mmmmn")]), 400.0, &style);
    assert_eq!(width, plain_width);
}

#[test]
fn a_reading_far_longer_than_its_base_widens_it_by_less_than_its_own_width() {
    let mut sh = TextShaper::new();
    let style = style().with_text_wrap(TextWrap::NoWrap);
    let long = "abcdefghij";
    let (plain_width, _) = sh.measure_text("i x", Some(&[plain(0..1)]), 400.0, &style);
    let (width, _) = sh.measure_text("i x", Some(&[reading(0..1, long)]), 400.0, &style);
    let half = style
        .clone()
        .with_font_size(style.font_size * style.ruby.size);
    let (nominal, _) = sh.measure_text(long, None, 400.0, &half);
    assert!(
        width > plain_width,
        "the base widens to hold what could not be fitted"
    );
    assert!(
        width - plain_width < nominal,
        "shrinking and tightening take part of it"
    );
}

#[test]
fn a_reading_is_drawn_above_its_base_in_the_room_its_line_made() {
    let mut sh = TextShaper::new();
    let style = style();
    let spans = [reading(0..4, "bs")];
    let (width, height) = sh.measure_text("base text", Some(&spans), 400.0, &style);
    let (_, plain_height) = sh.measure_text("base text", Some(&[plain(0..4)]), 400.0, &style);
    let rect = Rect::new(0.0, 0.0, width.ceil(), height.ceil());
    let room = (height - plain_height).round() as usize;
    let top_ink = |spans: &[Span], sh: &mut TextShaper| {
        let (pixels, w, h) = sh.rasterize("base text", Some(spans), rect, &style);
        (0..h as usize)
            .find(|row| (0..w as usize).any(|col| pixels[(row * w as usize + col) * 4 + 3] > 0))
            .expect("the text inks")
    };
    let base_top = top_ink(&[plain(0..4)], &mut sh) + room;
    assert!(room > 0);
    assert!(
        top_ink(&spans, &mut sh) < base_top,
        "ink above where the base's own begins"
    );
}

#[test]
fn a_reading_takes_the_ink_of_the_run_it_reads() {
    let mut sh = TextShaper::new();
    let style = style();
    let red = Color::rgba(1.0, 0.0, 0.0, 1.0);
    let base = Span::new(0..4, Declared::default().with_color(red)).with_ruby("bs");
    let (width, height) = sh.measure_text(
        "base text",
        Some(std::slice::from_ref(&base)),
        400.0,
        &style,
    );
    let rect = Rect::new(0.0, 0.0, width.ceil(), height.ceil());
    let mut glyphs = Vec::new();
    sh.layout_glyphs("base text", Some(&[base]), rect, &style, 1.0, &mut glyphs);
    let tinted = glyphs
        .iter()
        .filter(|glyph| glyph.color == red.to_array())
        .count();
    assert_eq!(
        tinted,
        "base".len() + "bs".len(),
        "the base and its reading are both red"
    );
}

#[test]
fn a_press_on_a_reading_answers_with_its_base() {
    let mut sh = TextShaper::new();
    let style = style();
    let spans = [plain(0..5), reading(5..9, "rd")];
    let text = "text base";
    let (width, _) = sh.measure_text("text ba", None, 400.0, &style);
    let at = sh.index_at(text, Some(&spans), 400.0, &style, (width, 1.0));
    assert!(matches!(at, Some(5..=8)), "{at:?}");
}

#[test]
fn a_press_after_a_base_still_finds_the_run_it_lands_on() {
    let mut sh = TextShaper::new();
    let style = style();
    let text = "base text";
    let spans = [reading(0..4, "bs"), plain(5..9)];
    let (_, height) = sh.measure_text(text, Some(&spans), 400.0, &style);
    let (before, _) = sh.measure_text("base te", None, 400.0, &style);
    let at = sh.index_at(
        text,
        Some(&spans),
        400.0,
        &style,
        (before + 1.0, height - 2.0),
    );
    assert_eq!(at, Some(7));
}
