use super::*;

use crate::{Color, Declared, TextCase, TextStyle, case_text};

fn reading(range: std::ops::Range<u32>, reading: &str) -> Span {
    Span::new(range, Declared::default()).with_ruby(reading)
}

#[test]
fn a_reading_that_fits_its_base_keeps_its_size() {
    let fit = RubyStyle::default().fit(20.0, 40.0, 30.0, 3);
    assert_eq!(fit.size, 10.0);
    assert_eq!(fit.tightening, 0.0);
    assert_eq!(fit.widening, 0.0);
}

#[test]
fn a_slightly_long_reading_shrinks_to_its_base_rather_than_widening_it() {
    let fit = RubyStyle::default().fit(20.0, 40.0, 42.0, 4);
    assert!(fit.size < 10.0 && fit.size >= 9.2, "{fit:?}");
    assert!((fit.width - 40.0).abs() < 1e-3, "{fit:?}");
    assert_eq!(fit.tightening, 0.0);
    assert_eq!(fit.widening, 0.0);
}

#[test]
fn a_reading_too_long_to_shrink_into_its_base_stops_at_its_smallest_size_and_tightens() {
    let style = RubyStyle::default();
    let fit = style.fit(20.0, 40.0, 46.0, 5);
    assert_eq!(fit.size, style.min_size * 20.0);
    assert!(fit.tightening > 0.0, "{fit:?}");
    assert!((fit.width - 40.0).abs() < 1e-3, "{fit:?}");
    assert_eq!(fit.widening, 0.0);
}

#[test]
fn only_what_neither_shrinking_nor_tightening_absorbs_widens_the_base() {
    let style = RubyStyle::default();
    let fit = style.fit(20.0, 20.0, 50.0, 5);
    assert_eq!(fit.size, style.min_size * 20.0);
    assert_eq!(fit.tightening, RubyStyle::TIGHTEST * fit.size);
    assert!(fit.widening > 0.0);
    assert!((fit.width - (20.0 + fit.widening)).abs() < 1e-3, "{fit:?}");
    assert!(fit.width < 50.0 * style.min_size / style.size);
}

#[test]
fn a_single_letter_reading_cannot_tighten() {
    let fit = RubyStyle::default().fit(20.0, 10.0, 30.0, 1);
    assert_eq!(fit.tightening, 0.0);
    assert!(fit.widening > 0.0);
}

#[test]
fn gluing_joins_the_clusters_of_each_base_and_nothing_else() {
    let spans = [
        reading(0..6, "kanji"),
        Span::new(9..12, Declared::default()),
    ];
    let glued = CasedText::as_written("漢字を読む", Some(&spans)).glued();
    assert_eq!(glued.text, "漢\u{2060}字を読む");
    let moved = glued.spans.as_deref().expect("spans");
    assert_eq!(moved[0].range, 0..9);
    assert_eq!(moved[0].ruby.as_deref(), Some("kanji"));
    assert_eq!(moved[1].range, 12..15);
}

#[test]
fn a_glued_index_answers_with_the_byte_it_was_written_at() {
    let spans = [reading(0..6, "kanji")];
    let glued = CasedText::as_written("漢字を", Some(&spans)).glued();
    assert_eq!(glued.source_index(0), 0);
    assert_eq!(
        glued.source_index(3),
        3,
        "the joiner stands where the second cluster begins"
    );
    assert_eq!(glued.source_index(6), 3);
    assert_eq!(glued.source_index(9), 6);
}

#[test]
fn a_paragraph_without_readings_is_borrowed_through() {
    let spans = [Span::new(0..3, Declared::default())];
    let glued = CasedText::as_written("abc", Some(&spans)).glued();
    assert!(matches!(glued.text, Cow::Borrowed(_)));
}

#[test]
fn inline_readings_follow_their_base_in_parentheses_inside_its_span() {
    let spans = [reading(0..6, "かんじ")];
    let inline = CasedText::as_written("漢字を", Some(&spans)).with_readings_inline();
    assert_eq!(inline.text, "漢字(かんじ)を");
    let moved = inline.spans.as_deref().expect("spans");
    assert_eq!(moved[0].range, 0..("漢字(かんじ)".len() as u32));
    assert_eq!(moved[0].ruby, None);
    let reading_at = "漢字(".len();
    assert_eq!(
        inline.source_index(reading_at),
        0,
        "a press on the reading is a press on its base"
    );
    assert_eq!(inline.source_index("漢字(かんじ)".len()), 6);
}

#[test]
fn readings_inline_compose_with_a_case() {
    let spans = [reading(0..3, "ker")];
    let style = TextStyle::new(14.0, Color::BLACK).with_text_case(TextCase::Upper);
    let inline = case_text("col nel", Some(&spans), &style).with_readings_inline();
    assert_eq!(inline.text, "COL(ker) NEL");
    assert_eq!(inline.source_index("COL(ker) ".len()), 4);
}
