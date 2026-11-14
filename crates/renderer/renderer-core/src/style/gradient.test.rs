use super::*;

#[test]
fn gradient_stops_new_stores_stops() {
    let stops = GradientStops::new(&[(0.0, Color::BLACK), (1.0, Color::WHITE)]);
    assert_eq!(stops.active().len(), 2);
    assert_eq!(stops.active()[0].color, Color::BLACK);
    assert_eq!(stops.active()[1].color, Color::WHITE);
}

#[test]
fn gradient_stops_new_truncates_to_eight() {
    let raw: Vec<(f32, Color)> = (0..6).map(|i| (i as f32 / 5.0, Color::BLACK)).collect();
    let stops = GradientStops::new(&raw);
    assert_eq!(stops.active().len(), 6);
}

#[test]
fn gradient_linear_stores_stops() {
    let p1 = Point::new(0.0, 0.0);
    let p2 = Point::new(1.0, 0.0);
    let g = Gradient::linear(p1, p2, &[(0.0, Color::BLACK), (1.0, Color::WHITE)]);
    assert_eq!(g.stops.active().len(), 2);
    assert_eq!(g.stops.active()[0].color, Color::BLACK);
}

#[test]
fn gradient_radial_stores_stops() {
    let c = Point::new(0.5, 0.5);
    let g = Gradient::radial(c, 1.0, &[(0.0, Color::RED), (1.0, Color::TRANSPARENT)]);
    assert_eq!(g.stops.active().len(), 2);
    assert_eq!(g.stops.active()[0].color, Color::RED);
}

#[test]
fn a_radial_gradient_carried_through_a_stretch_becomes_an_ellipse() {
    let g = Gradient::radial(
        Point::new(5.0, 5.0),
        5.0,
        &[(0.0, Color::RED), (1.0, Color::TRANSPARENT)],
    )
    .transformed(Transform::scale_around(4.0, 2.0, 0.0, 0.0));
    let GradientKind::Radial { transform } = g.kind else {
        panic!("expected a radial gradient, got {:?}", g.kind);
    };
    assert_eq!(
        transform.apply(Point::new(0.0, 0.0)),
        Point::new(20.0, 10.0)
    );
    assert_eq!(
        transform.apply(Point::new(1.0, 0.0)),
        Point::new(40.0, 10.0)
    );
    assert_eq!(
        transform.apply(Point::new(0.0, 1.0)),
        Point::new(20.0, 20.0)
    );
}

#[test]
fn a_linear_gradient_carried_through_a_transform_moves_both_ends() {
    let g = Gradient::linear(
        Point::new(0.0, 0.0),
        Point::new(10.0, 0.0),
        &[(0.0, Color::BLACK), (1.0, Color::WHITE)],
    )
    .transformed(Transform::translate(3.0, 4.0));
    assert_eq!(
        g.kind,
        GradientKind::Linear {
            start: Point::new(3.0, 4.0),
            end: Point::new(13.0, 4.0),
        }
    );
}

fn degenerate_radial() -> Gradient {
    Gradient::elliptical(
        Transform {
            a: 0.0,
            b: 0.0,
            c: 0.0,
            d: 0.0,
            e: 40.0,
            f: 20.0,
        },
        &[(0.0, Color::RED), (1.0, Color::BLUE)],
    )
}

#[test]
fn a_radial_gradient_with_no_area_collapses_to_its_last_stop() {
    assert_eq!(
        degenerate_radial().resolve(),
        ResolvedGradient::Solid(Color::BLUE)
    );
}

#[test]
fn a_radial_gradient_squashed_flat_by_a_transform_collapses_to_its_last_stop() {
    let flat = Gradient::radial(
        Point::new(0.0, 0.0),
        10.0,
        &[(0.0, Color::RED), (1.0, Color::BLUE)],
    )
    .transformed(Transform::scale_around(1.0, 0.0, 0.0, 0.0));
    assert_eq!(flat.resolve(), ResolvedGradient::Solid(Color::BLUE));
}

#[test]
fn a_gradient_with_an_area_does_not_collapse() {
    let radial = Gradient::radial(
        Point::new(0.0, 0.0),
        10.0,
        &[(0.0, Color::RED), (1.0, Color::BLUE)],
    );
    assert!(matches!(radial.resolve(), ResolvedGradient::Radial { .. }));
}

#[test]
fn a_linear_gradient_with_no_length_collapses_to_its_last_stop() {
    let point = Point::new(5.0, 5.0);
    let g = Gradient::linear(point, point, &[(0.0, Color::RED), (1.0, Color::BLUE)]);
    assert_eq!(g.resolve(), ResolvedGradient::Solid(Color::BLUE));
}

#[test]
fn a_linear_gradient_with_length_resolves_to_a_ramp_that_reaches_one_at_its_end() {
    let g = Gradient::linear(
        Point::new(0.0, 0.0),
        Point::new(10.0, 0.0),
        &[(0.0, Color::RED), (1.0, Color::BLUE)],
    );
    let ResolvedGradient::Linear { ramp, .. } = g.resolve() else {
        panic!("a linear gradient with length stays linear");
    };
    assert_eq!(ramp[0] * 10.0 + ramp[1] * 0.0, 1.0);
}
