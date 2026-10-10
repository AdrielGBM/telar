use super::*;

fn trigger_at(y: f32) -> Rect {
    Rect::new(10.0, y, 180.0, 36.0)
}

fn window(height: f32) -> Rect {
    Rect::new(0.0, 0.0, 400.0, height)
}

#[test]
fn a_panel_that_fits_opens_below_at_its_natural_height() {
    let spot = panel_spot(trigger_at(0.0), window(400.0), 100.0);
    assert_eq!((spot.top, spot.height), (36.0, 100.0));
}

#[test]
fn a_panel_taller_than_the_room_below_is_capped_to_it() {
    let spot = panel_spot(trigger_at(0.0), window(200.0), 800.0);
    assert_eq!(spot.top, 36.0);
    assert_eq!(spot.height, 200.0 - 36.0 - EDGE_MARGIN);
}

#[test]
fn a_panel_flips_above_when_there_is_more_room_there() {
    let spot = panel_spot(trigger_at(300.0), window(400.0), 250.0);
    assert_eq!(spot.height, 250.0);
    assert_eq!(spot.top, 300.0 - 250.0);
}

#[test]
fn a_flipped_panel_is_capped_to_the_room_above() {
    let spot = panel_spot(trigger_at(300.0), window(400.0), 800.0);
    assert_eq!(spot.height, 300.0 - EDGE_MARGIN);
    assert_eq!(spot.top, EDGE_MARGIN);
}

#[test]
fn an_unbounded_viewport_never_constrains() {
    let spot = panel_spot(trigger_at(0.0), unbounded(), 800.0);
    assert_eq!((spot.top, spot.height), (36.0, 800.0));
}
