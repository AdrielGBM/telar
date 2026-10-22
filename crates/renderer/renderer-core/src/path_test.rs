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
