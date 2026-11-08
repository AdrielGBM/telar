use super::*;
use crate::{Color, TextCase, TextStyle, case_text};

fn plain() -> TextStyle {
    TextStyle::new(14.0, Color::BLACK)
}

fn glyph(bytes: Range<usize>, x: f32, width: f32, level: u8) -> PlacedGlyph {
    PlacedGlyph {
        bytes,
        x,
        width,
        level,
    }
}

fn bytes_of(line: &TextLine) -> Vec<Range<usize>> {
    line.clusters.iter().map(|c| c.bytes.clone()).collect()
}

#[test]
fn a_ligature_is_shared_between_the_clusters_it_draws() {
    let text = "fi";
    let cased = case_text(text, None, &plain());
    let line = TextLine::from_glyphs(&cased, text, 0..2, false, 0.0, [glyph(0..2, 0.0, 10.0, 0)]);
    assert_eq!(bytes_of(&line), vec![0..1, 1..2]);
    assert!(line.clusters[0].x < line.clusters[1].x);
}

#[test]
fn the_glyphs_of_one_cluster_merge_into_it() {
    let text = "e\u{301}x";
    let cased = case_text(text, None, &plain());
    let glyphs = [
        glyph(0..1, 0.0, 8.0, 0),
        glyph(1..3, 2.0, 0.0, 0),
        glyph(3..4, 8.0, 8.0, 0),
    ];
    let line = TextLine::from_glyphs(&cased, text, 0..4, false, 0.0, glyphs);
    assert_eq!(bytes_of(&line), vec![0..3, 3..4]);
}

#[test]
fn a_right_to_left_ligature_puts_its_first_cluster_on_the_right() {
    let text = "\u{5d0}\u{5d1}";
    let cased = case_text(text, None, &plain());
    let line = TextLine::from_glyphs(&cased, text, 0..4, true, 10.0, [glyph(0..4, 0.0, 10.0, 1)]);
    assert_eq!(
        bytes_of(&line),
        vec![2..4, 0..2],
        "left to right as drawn, which is the reverse of the order written"
    );
    let first = &line.clusters[1];
    assert!(first.is_rtl());
    assert!(first.leading_x() > first.trailing_x());
}

#[test]
fn clusters_come_back_in_the_bytes_they_were_written_in() {
    let text = "stra\u{df}e";
    let style = plain().with_text_case(TextCase::Upper);
    let cased = case_text(text, None, &style);
    assert_eq!(cased.text, "STRASSE");
    let line = TextLine::left_to_right(&cased, text, 0..cased.text.len(), 0.0, |_| 5.0);
    assert_eq!(line.range, 0..text.len());
    let written: Vec<&str> = line
        .clusters
        .iter()
        .map(|c| &text[c.bytes.clone()])
        .collect();
    assert_eq!(written, vec!["s", "t", "r", "a", "\u{df}", "e"]);
    let sharp = &line.clusters[4];
    assert_eq!(sharp.width, 10.0, "both letters it is shown as");
}

#[test]
fn a_line_drawn_in_written_order_steps_by_cluster() {
    let text = "a\u{1f1ea}\u{1f1f8}\u{1f469}\u{200d}\u{1f4bb}";
    let cased = case_text(text, None, &plain());
    let line = TextLine::left_to_right(&cased, text, 0..text.len(), 3.0, |_| 4.0);
    assert_eq!(bytes_of(&line), vec![0..1, 1..9, 9..20]);
    assert_eq!(line.clusters[0].x, 3.0);
    assert!(line.clusters.iter().all(|c| !c.is_rtl()));
}
