//! A box's painted style, as the CSS that draws it.
//!
//! Separate from the layout half on purpose: layout comes from what the widget declared, paint comes from
//! the `Rect` the widget drew. The two reach a document backend by different routes and are only joined
//! here, in the string one element ends up with.

use renderer_core::{
    Border, BorderRadius, Color, Gradient, GradientKind, Paint, RectStyle, Shadow, TextStyle,
};

/// Appends `property: value;`.
pub fn declare(out: &mut String, property: &str, value: &str) {
    out.push_str(property);
    out.push(':');
    out.push_str(value);
    out.push(';');
}

pub fn color(c: Color) -> String {
    let [r, g, b, a] = c.to_rgba8();
    if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("rgba({r},{g},{b},{})", round(c.a))
    }
}

/// A number with at most three decimals, so an animated value does not churn the string every frame with
/// digits nobody can see.
pub fn round(value: f32) -> String {
    let rounded = (value * 1000.0).round() / 1000.0;
    if rounded.fract() == 0.0 {
        format!("{}", rounded as i64)
    } else {
        format!("{rounded}")
    }
}

pub fn px(value: f32) -> String {
    format!("{}px", round(value))
}

pub fn paint(p: &Paint) -> String {
    match p {
        Paint::Solid(c) => color(*c),
        Paint::Gradient(g) => gradient(g),
    }
}

fn gradient(g: &Gradient) -> String {
    let stops: Vec<String> = g
        .stops
        .active()
        .iter()
        .map(|stop| format!("{} {}%", color(stop.color), round(stop.position * 100.0)))
        .collect();
    let stops = stops.join(",");
    match g.kind {
        // The angle CSS wants is measured clockwise from "up", where the vector is measured from the
        // positive x axis with y growing downwards — hence the quarter turn and the sign.
        GradientKind::Linear { start, end } => {
            let (dx, dy) = (end.x - start.x, end.y - start.y);
            let degrees = dx.atan2(-dy).to_degrees().rem_euclid(360.0);
            format!("linear-gradient({}deg,{stops})", round(degrees))
        }
        GradientKind::Radial { radius, .. } => {
            format!("radial-gradient(circle {} at center,{stops})", px(radius))
        }
    }
}

fn radius(r: BorderRadius) -> Option<String> {
    if r.is_zero() {
        return None;
    }
    Some(
        if r.top_left == r.top_right && r.top_left == r.bottom_right && r.top_left == r.bottom_left
        {
            px(r.top_left)
        } else {
            format!(
                "{} {} {} {}",
                px(r.top_left),
                px(r.top_right),
                px(r.bottom_right),
                px(r.bottom_left)
            )
        },
    )
}

/// A box's frame, as the inset shadows that draw it.
///
/// Not `border`, which takes room: Telar's frame is paint on a box whose size layout already decided, and a
/// CSS border eats into the content box instead — a 1px frame left every child two pixels narrower than the
/// rect hit-testing reads. An inset shadow is drawn inside the same shape, follows the radius, and costs the
/// layout nothing.
fn border(b: &Border) -> Vec<String> {
    if !b.is_visible() {
        return Vec::new();
    }
    let colour = paint(&b.paint);
    let [top, right, bottom, left] = b.widths;
    if top == right && top == bottom && top == left {
        return vec![format!("inset 0 0 0 {} {colour}", px(top))];
    }
    // Each side is a shadow displaced by its own width, which fills exactly the band between the edge and
    // where it moved to. They overlap at the corners, which for a solid colour is the corner drawn twice.
    [
        (top > 0.0, format!("inset 0 {} 0 0 {colour}", px(top))),
        (right > 0.0, format!("inset {} 0 0 0 {colour}", px(-right))),
        (
            bottom > 0.0,
            format!("inset 0 {} 0 0 {colour}", px(-bottom)),
        ),
        (left > 0.0, format!("inset {} 0 0 0 {colour}", px(left))),
    ]
    .into_iter()
    .filter_map(|(drawn, shadow)| drawn.then_some(shadow))
    .collect()
}

fn shadow(s: Shadow) -> String {
    format!(
        "{} {} {} {} {}",
        px(s.offset_x),
        px(s.offset_y),
        px(s.blur_radius),
        px(s.spread),
        color(s.color)
    )
}

/// What a `Rect` the widget drew for its own box contributes to that box's style.
pub fn rect_style(style: &RectStyle, out: &mut String) {
    if let Some(fill) = &style.fill {
        declare(out, "background", &paint(fill));
    }
    if let Some(value) = radius(style.radius) {
        declare(out, "border-radius", &value);
    }
    // The frame and the shadow share one property, and the frame is drawn over the shadow.
    let mut shadows = style.border.as_ref().map(border).unwrap_or_default();
    shadows.extend(style.shadow.map(shadow));
    if !shadows.is_empty() {
        declare(out, "box-shadow", &shadows.join(","));
    }
}

/// What a `Text` command contributes to the element that holds it.
pub fn text_style(style: &TextStyle, out: &mut String) {
    declare(out, "font-size", &px(style.font_size));
    declare(out, "color", &paint(&style.color));
    if style.font_weight != 400 {
        declare(out, "font-weight", &style.font_weight.to_string());
    }
    if style.font_style != renderer_core::FontStyle::Normal {
        declare(out, "font-style", "italic");
    }
    if let renderer_core::FontFamily::Named(family) = &style.font_family {
        // Quoted, because a family name with a space in it is otherwise several identifiers.
        declare(out, "font-family", &format!("\"{family}\",sans-serif"));
    }
    match style.text_align {
        renderer_core::TextAlign::Start => {}
        renderer_core::TextAlign::Center => declare(out, "text-align", "center"),
        renderer_core::TextAlign::End => declare(out, "text-align", "end"),
        renderer_core::TextAlign::Justify => declare(out, "text-align", "justify"),
    }
    if let renderer_core::LineHeight::Times(factor) = style.line_height {
        declare(out, "line-height", &round(factor));
    }
    if style.letter_spacing != 0.0 {
        declare(out, "letter-spacing", &px(style.letter_spacing));
    }
    // Always said, because the document's default is not Telar's. `normal` collapses a run of spaces to one
    // and a newline to a space, so a paragraph written on several lines arrives as one — a source listing came
    // out as a single line thousands of characters wide. What Telar means is `pre-wrap`: the newlines an
    // author wrote are breaks, the spaces are spaces, and what does not fit still wraps.
    //
    // It is also what keeps the two engines agreeing on height. The measurer breaks on `\n` and counts the
    // lines; a document that collapsed them measured one line where layout had reserved twenty, and the
    // scroll area went on believing there was content below that the browser no longer had.
    declare(
        out,
        "white-space",
        if style.text_wrap == renderer_core::TextWrap::NoWrap {
            "pre"
        } else {
            "pre-wrap"
        },
    );
    if let Some(max) = style.clamp.max_lines() {
        // The only cross-browser line clamp there is, and it needs all four declarations to work.
        declare(out, "display", "-webkit-box");
        declare(out, "-webkit-box-orient", "vertical");
        declare(out, "-webkit-line-clamp", &max.to_string());
        declare(out, "overflow", "hidden");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use geometry_core::Point;
    use renderer_core::{Border, ShapeStyle};

    fn css_of(style: &RectStyle) -> String {
        let mut out = String::new();
        rect_style(style, &mut out);
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
}

#[cfg(test)]
mod text_tests {
    use super::*;
    use renderer_core::TextWrap;

    fn css_of(style: &TextStyle) -> String {
        let mut out = String::new();
        text_style(style, &mut out);
        out
    }

    /// The document's default collapses a newline to a space and a run of spaces to one; Telar's model does
    /// neither, and a paragraph written on several lines came out as a single line.
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
}
