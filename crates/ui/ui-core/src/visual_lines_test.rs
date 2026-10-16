use super::*;
use renderer_core::Color;

fn style() -> TextStyle {
    TextStyle::new(16.0, Color::BLACK)
}

fn width_of(text: &str) -> f32 {
    crate::text_metrics::measure_text(text, None, 1.0e6, &style()).0
}

#[test]
fn without_a_width_only_a_newline_ends_a_row() {
    let lines = VisualLines::of("abc abc abc\n\ncd", &style(), None);
    assert_eq!(lines.rows, vec![0..11, 12..12, 13..15]);
    assert!(!lines.wraps_after(0));
}

#[test]
fn nothing_is_one_empty_row() {
    assert_eq!(VisualLines::of("", &style(), Some(100.0)).rows, vec![0..0]);
}

#[test]
fn with_a_width_a_row_ends_where_the_measurer_wraps_it() {
    let width = width_of("abc abc") + 1.0;
    let lines = VisualLines::of("abc abc abc abc\nabc", &style(), Some(width));
    assert_eq!(lines.rows, vec![0..8, 8..15, 16..19]);
    assert!(lines.wraps_after(0));
    assert!(!lines.wraps_after(1));
}

#[test]
fn the_offset_a_wrap_shares_is_on_either_row_by_its_affinity() {
    let width = width_of("abc abc") + 1.0;
    let lines = VisualLines::of("abc abc abc abc\nabc", &style(), Some(width));
    assert_eq!(lines.row_of(8, Affinity::Downstream), 1);
    assert_eq!(lines.row_of(8, Affinity::Upstream), 0);
    assert_eq!(
        lines.row_of(16, Affinity::Upstream),
        2,
        "a newline is no wrap: the start of the row after it is only there"
    );
    assert_eq!(lines.row_of(19, Affinity::Downstream), 2);
}
