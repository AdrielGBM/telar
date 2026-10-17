use super::*;
use renderer_core::Color;

fn style() -> TextStyle {
    TextStyle::new(14.0, Color::BLACK)
}

fn advance(text: &str) -> f32 {
    crate::text_metrics::measure_text(text, None, 1.0e6, &style()).0
}

fn frame(width: f32) -> Frame {
    Frame {
        wrap: None,
        width,
        top: 0.0,
        scroll_x: 0.0,
        line_h: 20.0,
    }
}

#[test]
fn a_run_ending_in_a_space_is_as_wide_as_the_space_makes_it() {
    let rows = RowCache::default();
    let placed = Placed::new(&rows, "ab ", &style(), None, frame(200.0));
    assert!(placed.advance(0..3) > placed.advance(0..2));
}

#[test]
fn a_press_lands_on_the_nearest_boundary_and_names_the_letter_under_it() {
    let rows = RowCache::default();
    let text = "mañana";
    let placed = Placed::new(&rows, text, &style(), None, frame(200.0));
    let n = text.find('ñ').unwrap();
    let after = n + 'ñ'.len_utf8();
    let (start, end) = (advance(&text[..n]), advance(&text[..after]));
    let near_start = placed.hit(start + (end - start) * 0.3, 5.0);
    assert_eq!((near_start.at, near_start.under), (n, n));
    let near_end = placed.hit(start + (end - start) * 0.7, 5.0);
    assert_eq!((near_end.at, near_end.under), (after, n));
    let past = placed.hit(500.0, 5.0);
    assert_eq!(past.at, text.len());
    assert_eq!(
        past.under,
        text.len() - 1,
        "past the end the letter under it is the last"
    );
    assert_eq!(placed.hit(-20.0, 5.0).at, 0);
}

#[test]
fn a_press_below_the_text_lands_on_the_last_row_and_one_above_it_on_the_first() {
    let rows = RowCache::default();
    let placed = Placed::new(&rows, "uno\ndos", &style(), None, frame(200.0));
    assert_eq!(placed.row_at(-30.0), 0);
    assert_eq!(placed.row_at(25.0), 1);
    assert_eq!(placed.row_at(900.0), 1);
    assert_eq!(placed.hit(500.0, 900.0).at, 7);
}

#[test]
fn a_masked_text_is_measured_as_the_bullets_it_is_drawn_as() {
    let rows = RowCache::default();
    let placed = Placed::new(&rows, "mañana", &style(), Some('•'), frame(200.0));
    assert!((placed.advance(0..7) - advance("••••••")).abs() < 0.01);
}

#[test]
fn a_line_longer_than_the_box_can_be_scrolled_as_far_as_its_end_and_the_caret() {
    let rows = RowCache::default();
    let text = "a line much longer than the box it is in";
    let placed = Placed::new(&rows, text, &style(), None, frame(60.0));
    assert!((placed.max_scroll_x() - (advance(text) + CARET_WIDTH - 60.0)).abs() < 0.01);
    assert!((placed.text_width() - advance(text)).abs() < 0.01);
    let fits = Placed::new(&rows, "ab", &style(), None, frame(60.0));
    assert_eq!(fits.max_scroll_x(), 0.0);
    assert_eq!(fits.text_width(), 60.0);
}

#[test]
fn a_selection_paints_a_band_on_each_row_and_a_sliver_on_a_row_it_only_crosses() {
    let rows = RowCache::default();
    let placed = Placed::new(&rows, "uno\n\ntres", &style(), None, frame(200.0));
    let bands = placed.bands(1..7, true);
    assert_eq!(bands.len(), 3);
    assert_eq!(bands[1].0, 1);
    assert!(
        bands[1].2 > bands[1].1,
        "the empty row still shows the selection runs through it"
    );
}
