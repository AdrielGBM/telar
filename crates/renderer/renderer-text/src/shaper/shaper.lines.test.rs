//! The lines a shaped text is drawn in, as a caret walks them: grapheme clusters, in the order they are drawn, in the bytes they were written in.

use super::*;
use renderer_core::{TextCase, TextCluster, TextLine};

const TEST_FACE: &[u8] = include_bytes!("../../test-fonts/TelarTest.ttf");

fn style() -> TextStyle {
    let _installing = crate::fonts::FACES_STABLE
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let families = crate::fonts::install_face(TEST_FACE.to_vec())
        .expect("the bundled face must load, or text is drawn in whatever the machine has");
    TextStyle::new(16.0, Color::BLACK)
        .with_font_family(FontFamily::Named(families[0].as_str().into()))
}

fn one_line(text: &str, style: &TextStyle) -> TextLine {
    let mut lines = TextShaper::new().laid_lines(text, None, 400.0, style);
    assert_eq!(lines.len(), 1, "{text:?} is one line");
    lines.remove(0)
}

fn written<'a>(text: &'a str, line: &TextLine) -> Vec<&'a str> {
    line.clusters
        .iter()
        .map(|c| &text[c.bytes.clone()])
        .collect()
}

fn ascending(clusters: &[TextCluster]) -> bool {
    clusters.windows(2).all(|pair| pair[0].x <= pair[1].x)
}

#[test]
fn a_letter_and_the_marks_on_it_are_one_cluster() {
    let text = "e\u{301}x";
    let line = one_line(text, &style());
    assert_eq!(written(text, &line), vec!["e\u{301}", "x"]);
}

#[test]
fn a_flag_and_a_joined_emoji_are_one_cluster_each() {
    let text = "a\u{1f1ea}\u{1f1f8}\u{1f469}\u{200d}\u{1f4bb}b";
    let line = one_line(text, &style());
    assert_eq!(
        written(text, &line),
        vec!["a", "\u{1f1ea}\u{1f1f8}", "\u{1f469}\u{200d}\u{1f4bb}", "b"]
    );
}

#[test]
fn a_left_to_right_line_is_drawn_in_the_order_it_is_written() {
    let text = "abc";
    let line = one_line(text, &style());
    assert!(!line.rtl);
    assert_eq!(written(text, &line), vec!["a", "b", "c"]);
    assert!(ascending(&line.clusters));
    assert!(line.clusters.iter().all(|c| !c.is_rtl() && c.width > 0.0));
    assert_eq!(line.start_x, line.clusters[0].x);
}

#[test]
fn a_hebrew_line_is_drawn_from_the_right() {
    let text = "\u{5d0}\u{5d1}\u{5d2}";
    let line = one_line(text, &style());
    assert!(
        line.rtl,
        "a paragraph that starts in Hebrew reads right to left"
    );
    assert_eq!(
        written(text, &line),
        vec!["\u{5d2}", "\u{5d1}", "\u{5d0}"],
        "left to right as drawn, the reverse of the order written"
    );
    assert!(line.clusters.iter().all(TextCluster::is_rtl));
    let first = line.clusters.last().expect("three clusters");
    assert_eq!(
        line.start_x,
        first.leading_x(),
        "the line starts where its first letter does, on the right"
    );
}

#[test]
fn a_hebrew_word_inside_a_latin_line_is_reversed_in_place() {
    let text = "ab \u{5d0}\u{5d1} cd";
    let line = one_line(text, &style());
    assert!(!line.rtl);
    assert_eq!(
        written(text, &line),
        vec!["a", "b", " ", "\u{5d1}", "\u{5d0}", " ", "c", "d"]
    );
    let levels: Vec<bool> = line.clusters.iter().map(TextCluster::is_rtl).collect();
    assert_eq!(
        levels,
        vec![false, false, false, true, true, false, false, false]
    );
}

#[test]
fn a_line_that_does_not_wrap_is_still_aligned_in_its_box() {
    let start = one_line("ab", &style().with_text_wrap(TextWrap::NoWrap));
    let centred = one_line(
        "ab",
        &style()
            .with_text_wrap(TextWrap::NoWrap)
            .with_text_align(TextAlign::Center),
    );
    let end = one_line(
        "ab",
        &style()
            .with_text_wrap(TextWrap::NoWrap)
            .with_text_align(TextAlign::End),
    );
    assert!(start.start_x < centred.start_x && centred.start_x < end.start_x);
    let right = end.clusters.last().expect("two clusters");
    assert!((right.x + right.width - 400.0).abs() < 1.0);
}

#[test]
fn an_empty_paragraph_is_a_line_of_no_clusters_where_it_begins() {
    let text = "a\n\nb";
    let lines = TextShaper::new().laid_lines(text, None, 400.0, &style());
    let ranges: Vec<_> = lines.iter().map(|line| line.range.clone()).collect();
    assert_eq!(ranges, vec![0..1, 2..2, 3..4]);
    assert!(lines[1].clusters.is_empty());
    assert_eq!(lines[1].start_x, 0.0);
}

#[test]
fn a_cased_line_answers_in_the_bytes_it_was_written_in() {
    let text = "stra\u{df}e";
    let line = one_line(text, &style().with_text_case(TextCase::Upper));
    assert_eq!(
        written(text, &line),
        vec!["s", "t", "r", "a", "\u{df}", "e"]
    );
}
