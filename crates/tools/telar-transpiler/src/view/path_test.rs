use super::*;

#[test]
fn triangle_absolute() {
    let built = parse_path_data("M0,0 L100,0 L50,80 Z").unwrap();
    assert_eq!(
        built.chain,
        "PathData::new().move_to(Point::new(0.0, 0.0)).line_to(Point::new(100.0, 0.0)).line_to(Point::new(50.0, 80.0)).close()"
    );
    assert_eq!(built.max_x, 100.0);
    assert_eq!(built.max_y, 80.0);
}

#[test]
fn implicit_lineto_after_moveto() {
    let built = parse_path_data("M0 0 10 0 10 10").unwrap();
    assert_eq!(
        built.chain,
        "PathData::new().move_to(Point::new(0.0, 0.0)).line_to(Point::new(10.0, 0.0)).line_to(Point::new(10.0, 10.0))"
    );
}

#[test]
fn relative_commands_resolve_to_absolute() {
    let built = parse_path_data("m10,10 l10,0 l0,10 z").unwrap();
    assert_eq!(
        built.chain,
        "PathData::new().move_to(Point::new(10.0, 10.0)).line_to(Point::new(20.0, 10.0)).line_to(Point::new(20.0, 20.0)).close()"
    );
}

#[test]
fn horizontal_and_vertical() {
    let built = parse_path_data("M0,0 H50 V50 h-50 v-50 Z").unwrap();
    assert_eq!(
        built.chain,
        "PathData::new().move_to(Point::new(0.0, 0.0)).line_to(Point::new(50.0, 0.0)).line_to(Point::new(50.0, 50.0)).line_to(Point::new(0.0, 50.0)).line_to(Point::new(0.0, 0.0)).close()"
    );
}

#[test]
fn cubic_and_quadratic() {
    let built = parse_path_data("M0,0 C10,0 20,10 20,20 Q30,30 40,20").unwrap();
    assert!(
        built.chain.contains(
            ".cubic_to(Point::new(10.0, 0.0), Point::new(20.0, 10.0), Point::new(20.0, 20.0))"
        ),
        "{}",
        built.chain
    );
    assert!(
        built
            .chain
            .contains(".quad_to(Point::new(30.0, 30.0), Point::new(40.0, 20.0))"),
        "{}",
        built.chain
    );
}

#[test]
fn negative_and_decimal_no_separator() {
    let built = parse_path_data("M1.5.5L-3-4").unwrap();
    assert_eq!(
        built.chain,
        "PathData::new().move_to(Point::new(1.5, 0.5)).line_to(Point::new(-3.0, -4.0))"
    );
}

#[test]
fn missing_moveto_errors() {
    assert!(
        parse_path_data("L10,10").is_err(),
        "a path has to start somewhere"
    );
}

#[test]
fn unsupported_command_errors() {
    assert!(
        parse_path_data("M0,0 A10,10 0 0 1 20,20").is_err(),
        "an arc is not supported, and saying so beats emitting nothing"
    );
}
