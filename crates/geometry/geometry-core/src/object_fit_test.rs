use super::*;

#[test]
fn fill_returns_container_and_no_clip() {
    let c = Rect::new(0.0, 0.0, 120.0, 60.0);
    let (rect, clip) = fit_rect((10.0, 10.0), c, ObjectFit::Fill);
    assert_eq!(rect, c);
    assert!(!clip, "fill stretches to the box, so nothing spills out");
}

#[test]
fn contain_letterboxes_wide_box() {
    let c = Rect::new(0.0, 0.0, 120.0, 60.0);
    let (rect, clip) = fit_rect((10.0, 10.0), c, ObjectFit::Contain);
    assert_eq!(rect, Rect::new(30.0, 0.0, 60.0, 60.0));
    assert!(!clip, "contain fits inside the box, so nothing spills out");
}

#[test]
fn cover_overflows_and_clips() {
    let c = Rect::new(0.0, 0.0, 120.0, 60.0);
    let (rect, clip) = fit_rect((10.0, 10.0), c, ObjectFit::Cover);
    assert_eq!(rect, Rect::new(0.0, -30.0, 120.0, 120.0));
    assert!(clip, "cover overflows the box, so it has to be clipped");
}

#[test]
fn contain_respects_container_origin() {
    let c = Rect::new(5.0, 7.0, 120.0, 60.0);
    let (rect, _) = fit_rect((10.0, 10.0), c, ObjectFit::Contain);
    assert_eq!(rect, Rect::new(35.0, 7.0, 60.0, 60.0));
}

#[test]
fn degenerate_intrinsic_fills() {
    let c = Rect::new(0.0, 0.0, 120.0, 60.0);
    let (rect, clip) = fit_rect((0.0, 10.0), c, ObjectFit::Contain);
    assert_eq!(rect, c);
    assert!(!clip, "a degenerate intrinsic size falls back to filling");
}

#[test]
fn default_is_contain() {
    assert_eq!(ObjectFit::default(), ObjectFit::Contain);
}

/// The case `Contain` gets wrong for pixel art: at a fractional scale the source grid stops being a grid, because some pixels round to one screen pixel more than their neighbours.
#[test]
fn contain_integer_floors_the_scale_and_widens_the_letterbox() {
    // 320x180 into 1300x740: Contain would take min(4.06, 4.11) and smear the grid; flooring to 4 gives 1280x720 centred, spending the remainder on the border instead.
    let c = Rect::new(0.0, 0.0, 1300.0, 740.0);
    let (rect, clip) = fit_rect((320.0, 180.0), c, ObjectFit::ContainInteger);
    assert_eq!(rect, Rect::new(10.0, 10.0, 1280.0, 720.0));
    assert!(!clip, "an integer scale still fits inside the box");
}

/// An exact multiple has nothing to floor, so it must not lose a step to rounding.
#[test]
fn contain_integer_fills_an_exact_multiple() {
    let c = Rect::new(0.0, 0.0, 1280.0, 720.0);
    let (rect, _) = fit_rect((320.0, 180.0), c, ObjectFit::ContainInteger);
    assert_eq!(rect, Rect::new(0.0, 0.0, 1280.0, 720.0));
}

/// Below one there is no whole number to floor to, so it behaves as `Contain` rather than vanishing.
#[test]
fn contain_integer_below_one_falls_back_to_fitting() {
    let c = Rect::new(0.0, 0.0, 160.0, 90.0);
    let (rect, _) = fit_rect((320.0, 180.0), c, ObjectFit::ContainInteger);
    assert_eq!(rect, Rect::new(0.0, 0.0, 160.0, 90.0));
}
