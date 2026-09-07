use super::*;
use geometry_core::Point;
use renderer_core::TextWrap;

fn css_of(style: &TextStyle) -> String {
    let mut out = String::new();
    text_style(style, &mut out);
    out
}

/// The document's default collapses a newline to a space and a run of spaces to one; Telar's model does neither, and a paragraph written on several lines came out as a single line.
#[test]
fn the_lines_an_author_wrote_stay_lines() {
    assert!(
        css_of(&TextStyle::new(12.0, Color::BLACK)).contains("white-space:pre-wrap;"),
        "wrapping text has to keep its breaks and still wrap"
    );
}

#[test]
fn text_that_must_not_wrap_still_keeps_its_spaces() {
    let style = TextStyle::new(12.0, Color::BLACK).with_text_wrap(TextWrap::NoWrap);
    let css = css_of(&style);
    assert!(css.contains("white-space:pre;"), "{css}");
    assert!(!css.contains("pre-wrap"), "{css}");
}

/// `color` takes a colour, so a gradient written there was dropped and the glyphs came out in whatever they had inherited — the page's black, under a dark theme as much as a light one.
#[test]
fn glyphs_filled_with_a_gradient_are_a_background_clipped_to_them() {
    let gradient = Gradient::linear(
        Point::new(0.0, 0.0),
        Point::new(40.0, 0.0),
        &[(0.0, Color::BLACK), (1.0, Color::WHITE)],
    );
    let style = TextStyle::new(12.0, Paint::Gradient(gradient));
    let css = css_of(&style);
    assert!(
        !css.contains("color:linear-gradient"),
        "a colour property cannot carry a gradient: {css}"
    );
    assert!(css.contains("background-image:linear-gradient("), "{css}");
    assert!(css.contains("background-clip:text;"), "{css}");
    assert!(css.contains("color:transparent;"), "{css}");
    assert!(
        text_claims_background(&style),
        "and so it cannot share an element with a box that paints one"
    );
}

#[test]
fn a_colour_is_still_just_a_colour() {
    let style = TextStyle::new(12.0, Color::BLACK);
    assert!(
        css_of(&style).contains("color:#000000;"),
        "{}",
        css_of(&style)
    );
    assert!(
        !text_claims_background(&style),
        "a plain colour must not claim a background: {}",
        css_of(&style)
    );
}

/// Drawn on every other backend and on none of this one, so text that leaned on it for contrast had none.
#[test]
fn a_shadow_behind_the_glyphs_is_drawn() {
    let style = TextStyle::new(12.0, Color::WHITE).with_text_shadow(Shadow {
        offset_x: 0.0,
        offset_y: 1.0,
        blur_radius: 3.0,
        spread: 4.0,
        color: Color::BLACK,
    });
    let css = css_of(&style);
    assert!(
        css.contains("text-shadow:0px 1px 3px #000000;"),
        "and no spread, which `text-shadow` has no length for: {css}"
    );
}
