use super::*;
use geometry_core::CellSize;
use renderer_core::{Clamp, Color};

fn style() -> TextStyle {
    TextStyle::new(14.0, renderer_core::Paint::Solid(Color::WHITE))
}

fn metrics() -> CellMetrics {
    CellMetrics::new(CellSize::default())
}

#[test]
fn a_measured_box_lands_on_cell_boundaries() {
    let (w, h) = metrics().measure("hello", None, 1000.0, &style());
    assert_eq!(w, 5.0 * 8.0);
    assert_eq!(h, 16.0);
}

#[test]
fn wrapping_reports_the_extra_lines() {
    let (_, h) = metrics().measure("the quick brown fox", None, 10.0 * 8.0, &style());
    assert_eq!(h, 2.0 * 16.0);
}

#[test]
fn an_unbounded_probe_does_not_wrap() {
    let (w, h) = metrics().measure("the quick brown fox", None, 1.0e6, &style());
    assert_eq!(h, 16.0);
    assert_eq!(w, 19.0 * 8.0);
}

#[test]
fn a_clamp_caps_the_height() {
    let mut s = style();
    s.clamp = Clamp::lines(1, true);
    let (_, h) = metrics().measure("the quick brown fox", None, 10.0 * 8.0, &s);
    assert_eq!(h, 16.0);
}

#[test]
fn line_height_ignores_font_size() {
    assert_eq!(metrics().line_height(48.0), 16.0);
}

#[test]
fn min_content_is_the_widest_word_never_a_piece_of_one() {
    let (w, h) = metrics().min_content("go pricing now", None, &style());
    assert_eq!(w, 7.0 * 8.0);
    assert_eq!(h, 3.0 * 16.0);
}

#[test]
fn ink_starts_at_the_top_and_spans_the_rows_drawn() {
    assert_eq!(metrics().ink_bounds("hello", 1000.0, &style()), (0.0, 16.0));
    assert_eq!(
        metrics().ink_bounds("the quick brown fox", 10.0 * 8.0, &style()),
        (0.0, 32.0)
    );
}

#[test]
fn ink_skips_the_lines_a_clamp_hides() {
    let mut s = style();
    s.clamp = Clamp::lines(1, true);
    assert_eq!(
        metrics().ink_bounds("the quick brown fox", 10.0 * 8.0, &s),
        (0.0, 16.0)
    );
}

#[test]
fn a_paragraph_that_draws_nothing_has_no_ink() {
    assert_eq!(metrics().ink_bounds("", 1000.0, &style()), (0.0, 0.0));
    assert_eq!(metrics().ink_bounds("\n\n", 1000.0, &style()), (0.0, 0.0));
}

#[test]
fn line_starts_are_where_the_painter_breaks_each_line() {
    let text = "the quick brown fox";
    assert_eq!(
        metrics().line_starts(text, None, 10.0 * 8.0, &style()),
        vec![0, 10]
    );
    assert_eq!(
        metrics().line_starts("abcdefgh", None, 3.0 * 8.0, &style()),
        vec![0, 3, 6]
    );
    assert_eq!(
        metrics().line_starts("ab\ncd", None, 1.0e6, &style()),
        vec![0, 3]
    );
}

#[test]
fn line_starts_ignore_the_clamp() {
    let mut s = style();
    s.clamp = Clamp::lines(1, true);
    assert_eq!(
        metrics().line_starts("the quick brown fox", None, 10.0 * 8.0, &s),
        vec![0, 10]
    );
}

#[test]
fn a_laid_line_steps_by_cluster_in_written_order_and_a_wide_one_takes_two_cells() {
    let text = "a\u{65e5}e\u{301}\u{1f1ea}\u{1f1f8}";
    let lines = metrics().laid_lines(text, None, 1.0e6, &style());
    assert_eq!(lines.len(), 1);
    let clusters = &lines[0].clusters;
    let written: Vec<&str> = clusters.iter().map(|c| &text[c.bytes.clone()]).collect();
    assert_eq!(
        written,
        vec!["a", "\u{65e5}", "e\u{301}", "\u{1f1ea}\u{1f1f8}"]
    );
    assert_eq!(
        clusters[1].width,
        2.0 * 8.0,
        "an ideograph is two cells wide"
    );
    assert_eq!(clusters[2].x, 3.0 * 8.0);
    assert!(clusters.iter().all(|c| !c.is_rtl()));
}

#[test]
fn a_terminal_draws_hebrew_in_the_order_it_is_written() {
    let text = "\u{5d0}\u{5d1}";
    let line = metrics().laid_lines(text, None, 1.0e6, &style()).remove(0);
    assert!(!line.rtl);
    assert_eq!(line.clusters[0].bytes, 0..2);
    assert!(line.clusters[0].x < line.clusters[1].x);
}

#[test]
fn laid_lines_are_placed_as_the_painter_aligns_them_and_a_final_break_opens_a_line() {
    let centred = style().with_text_align(TextAlign::Center);
    let lines = metrics().laid_lines("ab\n", None, 10.0 * 8.0, &centred);
    let ranges: Vec<_> = lines.iter().map(|line| line.range.clone()).collect();
    assert_eq!(ranges, vec![0..2, 3..3]);
    assert_eq!(lines[0].start_x, 4.0 * 8.0);
    assert!(lines[1].clusters.is_empty());
}

#[test]
fn the_default_config_uses_the_shared_default_cell() {
    assert_eq!(crate::TuiConfig::default().cell, CellSize::default());
}
