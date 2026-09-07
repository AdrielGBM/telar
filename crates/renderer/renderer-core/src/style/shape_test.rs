use super::*;

fn box_100() -> Rect {
    Rect::new(0.0, 0.0, 100.0, 100.0)
}

#[test]
fn a_uniform_border_means_all_four_sides() {
    let style = RectStyle::default().with_border(Border::uniform(Color::BLACK, 2.0));
    assert_eq!(
        style.painted_border(),
        Some((Paint::Solid(Color::BLACK), [2.0; 4]))
    );
}

/// The whole point of the type: a colour with no side to paint it on draws nothing.
#[test]
fn every_side_at_zero_is_no_border_at_all() {
    let style =
        RectStyle::default().with_border(Border::per_side(Color::BLACK, 0.0, 0.0, 0.0, 0.0));
    assert_eq!(style.painted_border(), None);
}

#[test]
fn one_named_side_is_the_whole_frame() {
    let style =
        RectStyle::default().with_border(Border::per_side(Color::BLACK, 0.0, 0.0, 1.0, 0.0));
    assert_eq!(
        style.painted_border(),
        Some((Paint::Solid(Color::BLACK), [0.0, 0.0, 1.0, 0.0]))
    );
}

/// Thicknesses with no paint to draw them in used to be representable — a `border_widths` beside a `stroke: None`. Now there is nothing to construct.
#[test]
fn a_box_with_no_border_paints_none() {
    assert_eq!(RectStyle::default().painted_border(), None);
}

/// A side at zero leaves the inner edge flush with the outer one there, which is what makes the ring cover nothing along it — the rasterizer's two boundaries coincide and the shader's two SDFs agree.
#[test]
fn a_side_at_zero_leaves_the_inner_edge_flush_with_the_outer() {
    let (inner, _) =
        border_inner_shape(box_100(), BorderRadius::zero(), [0.0, 0.0, 1.0, 0.0]).unwrap();
    assert_eq!(inner, Rect::new(0.0, 0.0, 100.0, 99.0));
}

/// The uniform case has to come out exactly where the old single-stroke path put it: outer edge on the box, inner edge one width in, corners `r - w`.
#[test]
fn a_uniform_border_insets_every_side_and_tightens_every_corner() {
    let (inner, radius) = border_inner_shape(box_100(), BorderRadius::all(8.0), [2.0; 4]).unwrap();
    assert_eq!(inner, Rect::new(2.0, 2.0, 96.0, 96.0));
    assert_eq!(radius, BorderRadius::all(6.0));
}

/// Two sides of different thickness meet at a corner, and only one number is available to describe the arc between them.
#[test]
fn a_corner_is_tightened_by_the_thicker_of_the_two_sides_that_meet_there() {
    let (_, radius) =
        border_inner_shape(box_100(), BorderRadius::all(10.0), [1.0, 0.0, 0.0, 4.0]).unwrap();
    assert_eq!(radius.top_left, 6.0, "top 1, left 4 — the left side wins");
    assert_eq!(radius.top_right, 9.0, "top 1, right 0");
    assert_eq!(radius.bottom_right, 10.0, "neither side is drawn");
    assert_eq!(radius.bottom_left, 6.0, "bottom 0, left 4");
}

#[test]
fn a_corner_never_tightens_past_straight() {
    let (_, radius) = border_inner_shape(box_100(), BorderRadius::all(2.0), [8.0; 4]).unwrap();
    assert_eq!(radius, BorderRadius::zero());
}

/// The border is thicker than the box it frames, so there is no interior left to punch out and the caller paints the box solid instead.
#[test]
fn a_border_thicker_than_its_box_leaves_no_interior() {
    assert!(
        border_inner_shape(box_100(), BorderRadius::zero(), [60.0, 0.0, 60.0, 0.0]).is_none(),
        "borders wider than the box leave no interior"
    );
    assert!(
        border_inner_shape(box_100(), BorderRadius::zero(), [0.0, 50.0, 0.0, 50.0]).is_none(),
        "and neither do borders taller than it"
    );
}
