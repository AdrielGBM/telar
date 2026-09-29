use super::*;

#[test]
fn f32_lerp_midpoint() {
    assert!(
        (2.0f32.lerp(&4.0, 0.5) - 3.0).abs() < 1e-6,
        "{}",
        2.0f32.lerp(&4.0, 0.5)
    );
}

#[test]
fn point_lerp_is_component_wise() {
    let mid = Point::new(0.0, 10.0).lerp(&Point::new(4.0, 20.0), 0.5);
    assert_eq!(mid, Point::new(2.0, 15.0));
}

#[test]
fn rect_lerp_is_component_wise() {
    let mid = Rect::new(0.0, 0.0, 10.0, 20.0).lerp(&Rect::new(2.0, 4.0, 30.0, 40.0), 0.5);
    assert_eq!(mid, Rect::new(1.0, 2.0, 20.0, 30.0));
}

#[test]
fn border_radius_lerp_is_component_wise() {
    let mid = BorderRadius::all(0.0).lerp(&BorderRadius::all(8.0), 0.25);
    assert_eq!(mid, BorderRadius::all(2.0));
}

#[test]
fn color_lerp_endpoints_are_exact() {
    let red = Color::RED;
    let green = Color::GREEN;
    assert_eq!(red.lerp(&green, 0.0), red);
    assert_eq!(red.lerp(&green, 1.0), green);
}

#[test]
fn color_lerp_gray_to_gray_stays_neutral() {
    let mid = Color::rgb(0.2, 0.2, 0.2).lerp(&Color::rgb(0.8, 0.8, 0.8), 0.5);
    assert!((mid.r - mid.g).abs() < 2e-3, "{mid:?}");
    assert!((mid.g - mid.b).abs() < 2e-3, "{mid:?}");
}

#[test]
fn color_lerp_hue_takes_short_arc() {
    let mid = Color::RED.lerp(&Color::rgb(1.0, 1.0, 0.0), 0.5);
    let (_, chroma, hue, _) = mid.to_oklcha();
    assert!(
        chroma > ACHROMATIC_EPS,
        "midpoint unexpectedly gray: {mid:?}"
    );
    assert!(
        (29.0..=110.0).contains(&hue),
        "hue {hue} left the short arc"
    );
}

#[test]
fn color_lerp_achromatic_endpoint_carries_the_other_hue() {
    let mid = Color::rgb(0.5, 0.5, 0.5).lerp(&Color::RED, 0.5);
    let (_, chroma, hue, _) = mid.to_oklcha();
    let (_, _, red_hue, _) = Color::RED.to_oklcha();
    assert!(
        chroma > ACHROMATIC_EPS,
        "an achromatic endpoint must take the other end's hue, not grey out: {chroma}"
    );
    assert!(
        (hue - red_hue).abs() < 1.0,
        "hue {hue} != red hue {red_hue}"
    );
}

#[test]
fn font_axes_tween_and_spring_axis_by_axis() {
    use renderer_core::FontVariations;
    let from = FontVariations::new()
        .with("wght", 400.0)
        .with("wdth", 100.0);
    let to = FontVariations::new().with("wght", 800.0).with("wdth", 50.0);
    let half = Lerp::lerp(&from, &to, 0.5);
    assert_eq!(half.get("wght"), Some(600.0));
    assert_eq!(half.get("wdth"), Some(75.0));
    let delta = to.sub(&from);
    assert_eq!(delta.get("wght"), Some(400.0));
    assert_eq!(from.add(&delta), to);
    assert_eq!(delta.scale(0.5).get("wdth"), Some(-25.0));
    assert_eq!(FontVariations::zero().add(&from), from);
    assert_eq!(delta.magnitude_sq(), 400.0 * 400.0 + 50.0 * 50.0);
}
