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

const HEBREW: &str = "\u{5d0}\u{5d1}\u{5d2}";
const MIXED: &str = "ab \u{5d0}\u{5d1} cd";

fn caret_x(placed: &Placed<'_>, at: usize) -> f32 {
    placed.caret_rect(at, Affinity::Downstream).x
}

#[test]
fn a_hebrew_caret_walks_from_the_right_as_the_letters_are_drawn() {
    let rows = RowCache::default();
    let placed = Placed::new(&rows, HEBREW, &style(), None, frame(200.0));
    let stops: Vec<f32> = [0, 2, 4, 6].map(|at| caret_x(&placed, at)).to_vec();
    assert!(
        stops.windows(2).all(|pair| pair[0] > pair[1]),
        "each letter written moves the caret left: {stops:?}"
    );
}

#[test]
fn a_press_on_a_hebrew_letter_lands_on_the_side_it_is_nearest() {
    let rows = RowCache::default();
    let placed = Placed::new(&rows, HEBREW, &style(), None, frame(200.0));
    let (right, left) = (caret_x(&placed, 2), caret_x(&placed, 4));
    let near_right = placed.hit(left + (right - left) * 0.8, 5.0);
    assert_eq!((near_right.at, near_right.under), (2, 2));
    let near_left = placed.hit(left + (right - left) * 0.2, 5.0);
    assert_eq!((near_left.at, near_left.under), (4, 2));
    assert_eq!(
        placed.hit(199.0, 5.0).at,
        0,
        "the right end is where it begins"
    );
}

#[test]
fn a_caret_in_a_hebrew_word_inside_latin_text_is_placed_where_the_word_is_drawn() {
    let rows = RowCache::default();
    let placed = Placed::new(&rows, MIXED, &style(), None, frame(200.0));
    let (space, alef, bet, after) = (2, 3, 5, 7);
    assert!(caret_x(&placed, space) < caret_x(&placed, after));
    assert!(
        caret_x(&placed, alef) > caret_x(&placed, bet),
        "the first Hebrew letter is drawn to the right of the second"
    );
    for at in [0, 1, space, alef, bet, after, 8, 9, MIXED.len()] {
        let x = caret_x(&placed, at);
        let hit = placed.hit(x, 5.0);
        assert_eq!(
            caret_x(&placed, hit.at),
            x,
            "a press on the caret at {at} lands on a caret drawn there"
        );
    }
}

#[test]
fn a_selection_across_a_change_of_direction_is_painted_in_one_band_per_stretch() {
    let rows = RowCache::default();
    let placed = Placed::new(&rows, MIXED, &style(), None, frame(200.0));
    let bands = placed.bands(1..5, false);
    assert_eq!(bands.len(), 2, "{bands:?}");
    assert!(bands.iter().all(|(row, x0, x1)| *row == 0 && x0 < x1));
    assert!(
        bands[0].2 <= bands[1].1,
        "left to right and apart, with the unselected letter between them: {bands:?}"
    );
    let whole = placed.bands(0..MIXED.len(), false);
    assert_eq!(whole.len(), 1, "all of it is one stretch: {whole:?}");
}

#[test]
fn a_press_never_lands_inside_a_cluster() {
    let rows = RowCache::default();
    let text = "a\u{1f1ea}\u{1f1f8}e\u{301}\u{1f469}\u{200d}\u{1f4bb}";
    let placed = Placed::new(&rows, text, &style(), None, frame(200.0));
    let stops = [0, 1, 9, 12, text.len()];
    for x in (0..120).map(|x| x as f32) {
        let hit = placed.hit(x, 5.0);
        assert!(stops.contains(&hit.at), "{x} lands at {}", hit.at);
        assert!(stops.contains(&hit.under), "{x} is under {}", hit.under);
    }
}

#[test]
fn a_masked_text_draws_one_bullet_per_cluster_and_steps_over_them_whole() {
    let rows = RowCache::default();
    let text = "e\u{301}\u{1f1ea}\u{1f1f8}";
    assert_eq!(masked(text, '\u{2022}'), "\u{2022}\u{2022}");
    let placed = Placed::new(&rows, text, &style(), Some('\u{2022}'), frame(200.0));
    let x = caret_x(&placed, 3);
    assert!(caret_x(&placed, 0) < x && x < caret_x(&placed, text.len()));
    assert_eq!(placed.hit(x, 5.0).at, 3);
}
