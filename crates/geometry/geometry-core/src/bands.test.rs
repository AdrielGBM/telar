use super::*;

fn row(y: f32, height: f32) -> Rect {
    Rect::new(0.0, y, 100.0, height)
}

#[test]
fn a_line_finds_every_rect_that_reaches_it_in_list_order() {
    let bands = Bands::new([
        row(0.0, 20.0),
        row(500.0, 20.0),
        row(10.0, 300.0),
        row(1000.0, 1.0),
    ]);
    let found = bands.at(15.0);
    assert!(found.contains(&0) && found.contains(&2));
    assert!(!found.contains(&1) && !found.contains(&3));
    assert!(found.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn a_span_finds_each_rect_once() {
    let bands = Bands::new([row(0.0, 1000.0), row(300.0, 10.0)]);
    assert_eq!(bands.crossing(0.0, 900.0), vec![0, 1]);
}

#[test]
fn a_rect_far_down_a_long_page_is_found_only_near_it() {
    let rects: Vec<Rect> = (0..3000).map(|i| row(i as f32 * 40.0, 40.0)).collect();
    let bands = Bands::new(rects);
    let found = bands.at(2500.0 * 40.0 + 20.0);
    assert!(found.contains(&2500));
    assert!(found.len() <= 4, "{found:?}");
}

#[test]
fn a_rect_taller_than_the_bands_count_or_with_no_finite_edge_is_offered_everywhere() {
    let bands = Bands::new([row(0.0, 1.0e6), row(f32::NAN, 10.0), row(50.0, 10.0)]);
    assert_eq!(bands.at(900_000.0), vec![0, 1]);
    assert_eq!(bands.at(55.0), vec![0, 1, 2]);
}

#[test]
fn a_query_with_no_finite_edge_offers_every_rect() {
    let bands = Bands::new([row(0.0, 10.0), row(5000.0, 10.0)]);
    assert_eq!(bands.at(f32::INFINITY), vec![0, 1]);
}
