use super::*;
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
fn edges_round_so_neighbours_touch() {
    let cell = CellSize::default();
    let first = (cell.col_at(0.0), cell.col_at(37.5));
    let second = (cell.col_at(37.5), cell.col_at(75.0));
    assert_eq!(first.1, second.0, "a shared edge must land on one column");
}
