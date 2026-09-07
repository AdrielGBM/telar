use super::*;

#[test]
fn paint_from_color_creates_solid() {
    let color = Color::GREEN;
    let fill: Paint = color.into();
    assert_eq!(fill, Paint::Solid(color));
}

#[test]
fn line_cap_default_is_butt() {
    assert_eq!(LineCap::default(), LineCap::Butt);
}

#[test]
fn line_join_default_is_miter() {
    assert_eq!(LineJoin::default(), LineJoin::Miter);
}

#[test]
fn fill_rule_default_is_winding() {
    assert_eq!(FillRule::default(), FillRule::Winding);
}

#[test]
fn shadow_new_stores_fields_and_zero_spread() {
    let shadow = Shadow::new(2.0, 4.0, 8.0, Color::BLACK);
    assert_eq!(shadow.offset_x, 2.0);
    assert_eq!(shadow.offset_y, 4.0);
    assert_eq!(shadow.blur_radius, 8.0);
    assert_eq!(shadow.spread, 0.0);
    assert_eq!(shadow.color, Color::BLACK);
}

#[test]
fn shadow_with_spread_sets_spread() {
    let shadow = Shadow::new(0.0, 0.0, 4.0, Color::RED).with_spread(6.0);
    assert_eq!(shadow.spread, 6.0);
}

#[test]
fn stroke_new_stores_color_and_width() {
    let s = Stroke::new(Color::RED, 3.0);
    assert_eq!(s.paint, Paint::Solid(Color::RED));
    assert_eq!(s.width, 3.0);
}

#[test]
fn stroke_new_defaults_cap_to_butt() {
    let s = Stroke::new(Color::BLACK, 1.0);
    assert_eq!(s.cap, LineCap::Butt);
}

#[test]
fn stroke_new_defaults_join_to_miter() {
    let s = Stroke::new(Color::BLACK, 1.0);
    assert_eq!(s.join, LineJoin::Miter);
}

#[test]
fn stroke_with_cap_sets_cap() {
    let s = Stroke::new(Color::BLACK, 1.0).with_cap(LineCap::Square);
    assert_eq!(s.cap, LineCap::Square);
}

#[test]
fn stroke_with_join_sets_join() {
    let s = Stroke::new(Color::BLACK, 1.0).with_join(LineJoin::Bevel);
    assert_eq!(s.join, LineJoin::Bevel);
}
