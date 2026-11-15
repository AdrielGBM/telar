use super::*;
use crate::document::RESET;
use geometry_core::{Point, Transform};
use renderer_core::{Border, ShapeStyle};

fn css_of(style: &RectStyle) -> String {
    let mut out = String::new();
    rect_style(style, Rect::new(0.0, 0.0, 100.0, 40.0), &mut out);
    out
}

fn forced_colors() -> &'static str {
    let start = RESET
        .find("@media (forced-colors:active){")
        .expect("the sheet has a forced-colors block");
    &RESET[start..]
}

#[test]
fn an_even_frame_is_an_outline_that_takes_no_room_from_the_box() {
    let css = css_of(&RectStyle::default().with_border(Border::uniform(Color::BLACK, 1.0)));
    assert_eq!(css, "outline:1px solid #000000;outline-offset:-1px;");
    assert!(
        !css.contains("border"),
        "a CSS border would eat the content box"
    );
}

#[test]
fn an_uneven_frame_is_a_border_with_the_width_of_each_side() {
    let css = css_of(&RectStyle::default().with_border(Border::per_side(
        Color::BLACK,
        2.0,
        0.0,
        4.0,
        0.0,
    )));
    assert_eq!(
        css,
        "border-style:solid;border-color:#000000;border-width:2px 0px 4px 0px;"
    );
}

#[test]
fn a_frame_the_theme_wants_invisible_is_still_a_frame() {
    let css = css_of(&RectStyle::default().with_border(Border::uniform(Color::TRANSPARENT, 1.0)));
    assert_eq!(css, "outline:1px solid rgba(0,0,0,0);outline-offset:-1px;");
}

#[test]
fn a_frame_with_no_width_declares_nothing() {
    let css = css_of(&RectStyle::default().with_border(Border::uniform(Color::BLACK, 0.0)));
    assert_eq!(css, "");
}

#[test]
fn a_frame_and_a_shadow_are_declared_apart() {
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
        "outline:1px solid #000000;outline-offset:-1px;box-shadow:0px 2px 6px 0px #000000;"
    );
}

#[test]
fn forced_colors_paint_frames_in_the_text_colour() {
    let sheet = forced_colors();
    assert!(
        sheet.contains(
            "[data-telar] *{border-color:CanvasText!important;outline-color:CanvasText!important}"
        ),
        "{sheet}"
    );
}

#[test]
fn forced_colors_paint_a_chosen_box_as_a_highlight() {
    let sheet = forced_colors();
    for state in [
        "[aria-pressed=true]",
        "[aria-pressed=mixed]",
        "[aria-checked=true]",
        "[aria-checked=mixed]",
        "[aria-selected=true]",
    ] {
        assert!(sheet.contains(state), "{state} has no rule: {sheet}");
    }
    assert!(
        sheet.contains("{background:Highlight!important;color:HighlightText!important}"),
        "{sheet}"
    );
    assert!(
        sheet.contains(" *{color:HighlightText!important}"),
        "{sheet}"
    );
}

#[test]
fn forced_colors_paint_both_bands_of_the_focus_ring_as_a_highlight() {
    let sheet = forced_colors();
    for selector in [
        "[data-telar] :focus-visible{",
        "[data-telar] :focus-visible>:not([data-telar-id]){",
    ] {
        let rule = sheet
            .split_once(selector)
            .unwrap_or_else(|| panic!("{selector} has no rule: {sheet}"))
            .1;
        assert!(
            rule.starts_with("border-color:Highlight!important;outline-color:Highlight!important}"),
            "{selector}: {rule}"
        );
    }
}

#[test]
fn forced_colors_adjust_nothing_but_pictures() {
    let sheet = forced_colors();
    assert_eq!(sheet.matches("forced-color-adjust").count(), 1, "{sheet}");
    assert!(
        sheet.contains("[data-telar] :is(img,svg){forced-color-adjust:none}"),
        "{sheet}"
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

/// A gradient is not a colour an `outline` takes, so the frame is a background layer and the outline it leaves is transparent, which forced colours can still repaint.
#[test]
fn a_gradient_frame_is_a_layer_with_a_transparent_outline_beside_the_drop_shadow() {
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
        "the shadow is declared whole: {css}"
    );
    assert!(
        css.contains("background:url(\"data:image/svg+xml,"),
        "the frame is drawn as the ring it is: {css}"
    );
    assert!(
        css.contains("outline:2px solid rgba(0,0,0,0);"),
        "forced colours still find a frame to repaint: {css}"
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
        .and_then(|rest| rest.split_once(";outline:").map(|(layers, _)| layers))
        .unwrap_or_else(|| panic!("one background property, then the frame: {css}"));
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

#[test]
fn an_elliptical_radial_fill_keeps_both_radii() {
    let css = css_of(
        &RectStyle::default().with_fill(Paint::Gradient(Gradient::elliptical(
            Transform {
                a: 40.0,
                b: 0.0,
                c: 0.0,
                d: 20.0,
                e: 40.0,
                f: 20.0,
            },
            &[(0.0, Color::BLACK), (1.0, Color::WHITE)],
        ))),
    );
    assert!(
        css.contains("radial-gradient(40px 20px at center,#000000 0%,#ffffff 100%)"),
        "{css}"
    );
}

#[test]
fn a_radial_fill_with_no_area_is_its_last_stop() {
    let css = css_of(
        &RectStyle::default().with_fill(Paint::Gradient(Gradient::radial(
            Point::new(40.0, 20.0),
            0.0,
            &[(0.0, Color::RED), (1.0, Color::BLUE)],
        ))),
    );
    assert!(css.contains(&color(Color::BLUE)), "{css}");
    assert!(!css.contains("gradient"), "{css}");
    assert!(!css.contains(&color(Color::RED)), "{css}");
}

#[test]
fn a_linear_fill_with_no_length_is_its_last_stop() {
    let point = Point::new(40.0, 20.0);
    let css = css_of(
        &RectStyle::default().with_fill(Paint::Gradient(Gradient::linear(
            point,
            point,
            &[(0.0, Color::RED), (1.0, Color::BLUE)],
        ))),
    );
    assert!(css.contains(&color(Color::BLUE)), "{css}");
    assert!(!css.contains("gradient"), "{css}");
    assert!(!css.contains(&color(Color::RED)), "{css}");
}
