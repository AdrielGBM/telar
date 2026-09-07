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
