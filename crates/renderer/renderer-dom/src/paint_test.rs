use super::*;
use geometry_core::Point;
use renderer_core::{Border, ShapeStyle};

fn css_of(style: &RectStyle) -> String {
    let mut out = String::new();
    rect_style(style, Rect::new(0.0, 0.0, 100.0, 40.0), &mut out);
    out
}

#[test]
fn a_frame_takes_no_room_from_the_box_it_is_drawn_on() {
    let css = css_of(&RectStyle::default().with_border(Border::uniform(Color::BLACK, 1.0)));
    assert_eq!(css, "box-shadow:inset 0 0 0 1px #000000;");
    assert!(
        !css.contains("border:"),
        "a CSS border would eat the content box"
    );
}

#[test]
fn each_side_of_an_uneven_frame_is_drawn_on_its_own() {
    let css = css_of(&RectStyle::default().with_border(Border::per_side(
        Color::BLACK,
        2.0,
        0.0,
        4.0,
        0.0,
    )));
    assert_eq!(
        css,
        "box-shadow:inset 0 2px 0 0 #000000,inset 0 -4px 0 0 #000000;"
    );
}

#[test]
fn a_frame_and_a_shadow_share_one_property_with_the_frame_on_top() {
    let css = css_of(
        &RectStyle::default()
            .with_border(Border::uniform(Color::BLACK, 1.0))
            .with_shadow(Shadow {
                offset_x: 0.0,
                offset_y: 2.0,
                blur_radius: 6.0,
                spread: 0.0,
                color: Color::BLACK,
            }),
    );
    assert_eq!(
        css,
        "box-shadow:inset 0 0 0 1px #000000,0px 2px 6px 0px #000000;"
    );
}

#[test]
fn a_fill_and_a_radius_are_what_they_look_like() {
    let css = css_of(
        &RectStyle::default()
            .with_fill(Color::WHITE)
            .with_radius(BorderRadius::all(6.0)),
    );
    assert_eq!(css, "background:#ffffff;border-radius:6px;");
}

#[test]
fn a_gradient_fill_is_measured_from_where_it_points() {
    let css = css_of(
        &RectStyle::default().with_fill(Paint::Gradient(Gradient::linear(
            Point::new(0.0, 0.0),
            Point::new(0.0, 10.0),
            &[(0.0, Color::BLACK), (1.0, Color::WHITE)],
        ))),
    );
    assert!(
        css.contains("linear-gradient(180deg,#000000 0%,#ffffff 100%)"),
        "{css}"
    );
}

#[test]
fn a_transparent_colour_keeps_its_alpha() {
    assert_eq!(color(Color::rgba(0.0, 0.0, 0.0, 0.5)), "rgba(0,0,0,0.5)");
}

/// The whole point of telling the browser which scheme a surface is: the two are told apart by how bright the colour looks, not by which channel happens to be largest.
#[test]
fn a_surface_is_the_scheme_its_own_brightness_makes_it() {
    assert_eq!(scheme_of(Color::from_hex("#0e1017").unwrap()), "dark");
    assert_eq!(scheme_of(Color::from_hex("#f6f7fb").unwrap()), "light");
    // A saturated blue and a saturated yellow average the same and are nothing alike to look at.
    assert_eq!(scheme_of(Color::rgba(0.0, 0.0, 1.0, 1.0)), "dark");
    assert_eq!(scheme_of(Color::rgba(1.0, 1.0, 0.0, 1.0)), "light");
}

/// A gradient is not a colour, and `box-shadow` takes one: written there the declaration was invalid and the browser dropped it whole — the frame *and* the drop shadow that shares the property.
#[test]
fn a_gradient_frame_does_not_take_the_drop_shadow_down_with_it() {
    let gradient = Gradient::linear(
        Point::new(0.0, 0.0),
        Point::new(100.0, 0.0),
        &[(0.0, Color::BLACK), (1.0, Color::WHITE)],
    );
    let css = css_of(
        &RectStyle::default()
            .with_border(Border::uniform(Paint::Gradient(gradient), 2.0))
            .with_shadow(Shadow {
                offset_x: 0.0,
                offset_y: 2.0,
                blur_radius: 6.0,
                spread: 0.0,
                color: Color::BLACK,
            }),
    );
    assert!(
        css.contains("box-shadow:0px 2px 6px 0px #000000;"),
        "the shadow is the whole of the property, and valid: {css}"
    );
    assert!(
        css.contains("background:url(\"data:image/svg+xml,"),
        "the frame is drawn as the ring it is: {css}"
    );
}

/// A frame over a fill, in one property — and the colour last, which is the only layer it may be.
#[test]
fn a_gradient_frame_is_a_layer_over_the_fill() {
    let gradient = Gradient::linear(
        Point::new(0.0, 0.0),
        Point::new(100.0, 0.0),
        &[(0.0, Color::BLACK), (1.0, Color::WHITE)],
    );
    let css = css_of(
        &RectStyle::default()
            .with_fill(Color::WHITE)
            .with_border(Border::uniform(Paint::Gradient(gradient), 2.0)),
    );
    let background = css
        .strip_prefix("background:")
        .and_then(|rest| rest.strip_suffix(';'))
        .unwrap_or_else(|| panic!("one background property and nothing else: {css}"));
    assert!(background.starts_with("url(\""), "{background}");
    assert!(background.ends_with(",#ffffff"), "{background}");
}

/// `#` starts a fragment and `<` is not a character a URL carries; a colour or a tag written raw ended the picture early and the frame did not draw at all.
#[test]
fn a_picture_is_escaped_where_a_url_cannot_carry_it() {
    let uri = data_uri("<svg fill=\"#abc\"/>");
    assert_eq!(
        uri,
        "url(\"data:image/svg+xml,%3Csvg fill=%22%23abc%22/%3E\")"
    );
}
