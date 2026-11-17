use super::*;

#[test]
fn path_data_new_is_empty() {
    let path = PathData::new();
    assert!(path.verbs().is_empty(), "a new path has no verbs");
}

#[test]
fn path_data_move_to_adds_verb() {
    let path = PathData::new().move_to(Point::new(0.0, 0.0));
    assert_eq!(path.verbs().len(), 1);
    assert!(
        matches!(path.verbs()[0], PathVerb::MoveTo(_)),
        "{:?}",
        path.verbs()
    );
}

#[test]
fn path_data_line_to_adds_verb() {
    let path = PathData::new()
        .move_to(Point::new(0.0, 0.0))
        .line_to(Point::new(1.0, 1.0));
    assert_eq!(path.verbs().len(), 2);
    assert!(
        matches!(path.verbs()[1], PathVerb::LineTo(_)),
        "{:?}",
        path.verbs()
    );
}

#[test]
fn path_data_close_adds_verb() {
    let path = PathData::new().move_to(Point::new(0.0, 0.0)).close();
    assert!(
        matches!(path.verbs().last().unwrap(), PathVerb::Close),
        "{:?}",
        path.verbs()
    );
}

#[test]
fn path_data_quad_to_adds_verb() {
    let path = PathData::new().quad_to(Point::new(1.0, 0.0), Point::new(2.0, 0.0));
    assert!(
        matches!(path.verbs()[0], PathVerb::QuadTo { .. }),
        "{:?}",
        path.verbs()
    );
}

#[test]
fn path_data_cubic_to_adds_verb() {
    let path = PathData::new().cubic_to(
        Point::new(1.0, 0.0),
        Point::new(2.0, 0.0),
        Point::new(3.0, 0.0),
    );
    assert!(
        matches!(path.verbs()[0], PathVerb::CubicTo { .. }),
        "{:?}",
        path.verbs()
    );
}

#[test]
fn path_data_builder_accumulates_verbs() {
    let path = PathData::new()
        .move_to(Point::new(0.0, 0.0))
        .line_to(Point::new(1.0, 0.0))
        .line_to(Point::new(1.0, 1.0))
        .close();
    assert_eq!(path.verbs().len(), 4);
}

fn triangle() -> PathData {
    PathData::new()
        .move_to(Point::new(0.0, 0.0))
        .line_to(Point::new(4.0, 0.0))
        .line_to(Point::new(2.0, 3.0))
        .close()
}

#[test]
fn same_verbs_have_the_same_id() {
    assert_eq!(triangle().id(), triangle().id());
}

#[test]
fn id_does_not_depend_on_creation_order() {
    let first = triangle();
    let other = PathData::new().move_to(Point::new(9.0, 9.0));
    let second = triangle();
    assert_ne!(other.id(), first.id());
    assert_eq!(first.id(), second.id());
}

#[test]
fn a_diverging_clone_gets_a_different_id() {
    let original = triangle();
    let before = original.id();
    let extended = original.clone().line_to(Point::new(1.0, 1.0));
    assert_ne!(extended.id(), before);
    assert_eq!(original.id(), before);
}

#[test]
fn extending_after_reading_the_id_refreshes_it() {
    let path = triangle();
    let before = path.id();
    assert_ne!(path.line_to(Point::new(1.0, 1.0)).id(), before);
}

#[test]
fn verb_kind_and_coordinates_change_the_id() {
    let line = PathData::new()
        .move_to(Point::new(0.0, 0.0))
        .line_to(Point::new(1.0, 1.0));
    let moved = PathData::new()
        .move_to(Point::new(0.0, 0.0))
        .move_to(Point::new(1.0, 1.0));
    let shifted = PathData::new()
        .move_to(Point::new(0.0, 0.0))
        .line_to(Point::new(1.0, 2.0));
    assert_ne!(line.id(), moved.id());
    assert_ne!(line.id(), shifted.id());
}

#[test]
fn negative_zero_hashes_like_zero() {
    let positive = PathData::new()
        .move_to(Point::new(0.0, 0.0))
        .line_to(Point::new(1.0, 0.0));
    let negative = PathData::new()
        .move_to(Point::new(-0.0, -0.0))
        .line_to(Point::new(1.0, -0.0));
    assert_eq!(positive, negative);
    assert_eq!(positive.id(), negative.id());
}

fn ring() -> PathData {
    let mut path = PathData::new();
    for inset in [0.0, 10.0] {
        let (lo, hi) = (inset, 40.0 - inset);
        path = path
            .move_to(Point::new(lo, lo))
            .line_to(Point::new(hi, lo))
            .line_to(Point::new(hi, hi))
            .line_to(Point::new(lo, hi))
            .close();
    }
    path
}

#[test]
fn a_point_is_inside_a_filled_polygon_and_outside_past_its_edges() {
    let triangle = PathData::polygon(&[
        Point::new(0.0, 0.0),
        Point::new(40.0, 0.0),
        Point::new(0.0, 40.0),
    ]);
    assert!(triangle.contains(Point::new(10.0, 10.0), FillRule::Winding));
    assert!(!triangle.contains(Point::new(30.0, 30.0), FillRule::Winding));
    assert!(!triangle.contains(Point::new(-5.0, 10.0), FillRule::Winding));
}

#[test]
fn the_fill_rule_decides_the_hole_of_two_nested_squares_wound_the_same_way() {
    let centre = Point::new(20.0, 20.0);
    assert!(ring().contains(centre, FillRule::Winding));
    assert!(!ring().contains(centre, FillRule::EvenOdd));
    assert!(ring().contains(Point::new(5.0, 20.0), FillRule::EvenOdd));
}

#[test]
fn a_curved_edge_is_followed_and_an_open_subpath_fills_as_if_closed() {
    let dome = PathData::new().move_to(Point::new(0.0, 40.0)).cubic_to(
        Point::new(0.0, -13.0),
        Point::new(40.0, -13.0),
        Point::new(40.0, 40.0),
    );
    assert!(dome.contains(Point::new(20.0, 10.0), FillRule::Winding));
    assert!(!dome.contains(Point::new(2.0, 2.0), FillRule::Winding));
    let bowl = PathData::new()
        .move_to(Point::new(0.0, 0.0))
        .quad_to(Point::new(20.0, 80.0), Point::new(40.0, 0.0));
    assert!(bowl.contains(Point::new(20.0, 20.0), FillRule::Winding));
    assert!(!bowl.contains(Point::new(20.0, 45.0), FillRule::Winding));
}

fn corners(path: &PathData) -> Vec<Point> {
    path.verbs()
        .iter()
        .filter_map(|verb| match verb {
            PathVerb::MoveTo(p) | PathVerb::LineTo(p) => Some(*p),
            _ => None,
        })
        .collect()
}

#[test]
fn a_fitted_path_scales_its_view_box_to_fill_the_box() {
    let square = PathData::polygon(&[
        Point::new(0.0, 0.0),
        Point::new(24.0, 0.0),
        Point::new(24.0, 24.0),
    ]);
    let fitted = square.fitted(
        Rect::new(0.0, 0.0, 24.0, 24.0),
        Rect::new(0.0, 0.0, 48.0, 48.0),
    );
    assert_eq!(
        corners(&fitted),
        vec![
            Point::new(0.0, 0.0),
            Point::new(48.0, 0.0),
            Point::new(48.0, 48.0)
        ]
    );
}

#[test]
fn a_fitted_path_keeps_its_aspect_and_is_centred_on_the_longer_side() {
    let line = PathData::new()
        .move_to(Point::new(10.0, 10.0))
        .line_to(Point::new(20.0, 20.0));
    let fitted = line.fitted(
        Rect::new(10.0, 10.0, 10.0, 10.0),
        Rect::new(0.0, 0.0, 40.0, 20.0),
    );
    assert_eq!(
        corners(&fitted),
        vec![Point::new(10.0, 0.0), Point::new(30.0, 20.0)],
        "scaled by the shorter side and centred along the longer one"
    );
}

#[test]
fn a_view_box_with_no_area_leaves_the_path_as_drawn() {
    let fitted = triangle().fitted(
        Rect::new(0.0, 0.0, 0.0, 10.0),
        Rect::new(0.0, 0.0, 40.0, 40.0),
    );
    assert_eq!(fitted, triangle());
}
