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
