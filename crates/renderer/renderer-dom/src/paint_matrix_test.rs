use super::*;

/// A spinner: a quarter turn about its own centre, written by a widget as a turn about that centre's place on the page. Unrebased it read as "turn, then walk to where you already are", which is a spinner orbiting the window instead of turning in it.
#[test]
fn a_turn_about_a_box_far_down_the_page_stays_on_the_box() {
    // A 24x24 box at (300, 500): a 90 degree turn about (312, 512).
    let turn = [0.0, 1.0, -1.0, 0.0, 312.0 + 512.0, 512.0 - 312.0];
    let css = matrix(turn, 300.0, 500.0);
    // In the element's own coordinates the same turn is about (12, 12).
    assert_eq!(css, "matrix(0,1,-1,0,24,0)");
}

/// A slider's fill: scaled from its left edge, which is what makes it grow rightwards.
#[test]
fn a_scale_about_an_edge_keeps_that_edge() {
    // Half width about x = 200: x' = 0.5x + 100.
    let half = [0.5, 0.0, 0.0, 1.0, 100.0, 0.0];
    assert_eq!(matrix(half, 200.0, 40.0), "matrix(0.5,0,0,1,0,0)");
}

/// What a scroll area emits. Rebasing must leave it exactly as it was, or every scrolled panel moves.
#[test]
fn a_plain_translation_is_the_same_wherever_it_is_measured_from() {
    let scrolled = [1.0, 0.0, 0.0, 1.0, 0.0, -250.0];
    assert_eq!(
        matrix(scrolled, 640.0, 480.0),
        "matrix(1,0,0,1,0,-250)",
        "a translation carries no anchor to get wrong"
    );
}
