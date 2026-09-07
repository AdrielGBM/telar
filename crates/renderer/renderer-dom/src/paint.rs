//! A box's painted style, as the CSS that draws it.
//!
//! Separate from the layout half on purpose: layout comes from what the widget declared, paint comes from the `Rect` the widget drew. The two reach a document backend by different routes and are only joined here, in the string one element ends up with.

use geometry_core::Rect;
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

/// A colour as the CSS `rgba(...)` a declaration takes.
pub fn color(c: Color) -> String {
    let [r, g, b, a] = c.to_rgba8();
    if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("rgba({r},{g},{b},{})", round(c.a))
    }
}

/// Which of the two schemes a surface of this colour belongs to, as CSS `color-scheme` spells it.
///
/// Weighted by what each channel contributes to how bright a colour looks rather than averaged: a saturated blue and a saturated yellow average the same and are nothing alike to look at, and a theme built on either would have been handed the wrong one.
pub fn scheme_of(c: Color) -> &'static str {
    if 0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b < 0.5 {
        "dark"
    } else {
        "light"
    }
}

/// A number with at most three decimals, so an animated value does not churn the string every frame with digits nobody can see.
pub fn round(value: f32) -> String {
    let rounded = (value * 1000.0).round() / 1000.0;
    if rounded.fract() == 0.0 {
        format!("{}", rounded as i64)
    } else {
        format!("{rounded}")
    }
}

/// A length as a CSS `px` string, trimmed of trailing zeros.
pub fn px(value: f32) -> String {
    format!("{}px", round(value))
}

/// A paint as the CSS value a `background` or `color` takes.
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
        // CSS measures the angle clockwise from up, where the vector is measured from the positive x axis with y growing downwards — hence the quarter turn and the sign.
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
/// Not `border`, which takes room: Telar's frame is paint on a box whose size layout already decided, and a CSS border eats into the content box instead — a 1px frame left every child two pixels narrower than the rect hit-testing reads. An inset shadow is drawn inside the same shape, follows the radius, and costs the layout nothing.
fn border(b: &Border) -> Vec<String> {
    if !b.is_visible() {
        return Vec::new();
    }
    // A shadow carries a colour and a gradient is not one: written here it made the whole `box-shadow` invalid and the browser dropped it, taking the box's drop shadow with it, since the two share the property. A frame painted with a gradient becomes a background layer instead.
    let Paint::Solid(colour) = b.paint else {
        return Vec::new();
    };
    let colour = color(colour);
    let [top, right, bottom, left] = b.widths;
    if top == right && top == bottom && top == left {
        return vec![format!("inset 0 0 0 {} {colour}", px(top))];
    }
    // Each side is a shadow displaced by its own width, filling the band between the edge and where it moved to. They overlap at the corners, which for a solid colour is the corner drawn twice.
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

/// A picture, as the CSS that names it.
///
/// Only what a URL cannot carry raw is escaped: `%` because it introduces an escape, `#` because it would start a fragment, and the three the markup itself is made of.
fn data_uri(svg: &str) -> String {
    let mut out = String::from("url(\"data:image/svg+xml,");
    for character in svg.chars() {
        match character {
            '%' => out.push_str("%25"),
            '#' => out.push_str("%23"),
            '<' => out.push_str("%3C"),
            '>' => out.push_str("%3E"),
            '"' => out.push_str("%22"),
            _ => out.push(character),
        }
    }
    out.push_str("\")");
    out
}

/// A gradient frame, as the background layer that draws it — `None` for every frame [`border`] can draw itself.
///
/// Laid over the fill rather than beside it because a frame is over the box it frames, and sized to the box so the ring inside the picture lands exactly on its edge.
fn frame_image(style: &RectStyle, rect: Rect) -> Option<String> {
    let border = style.border.as_ref().filter(|b| b.is_visible())?;
    let Paint::Gradient(gradient) = &border.paint else {
        return None;
    };
    let svg = crate::vector::frame_svg(rect, style.radius, border.widths, gradient);
    Some(format!("{} 0 0/100% 100% no-repeat", data_uri(&svg)))
}

/// What a `Rect` the widget drew for its own box contributes to that box's style.
///
/// `rect` is the one it was drawn with, which is what a gradient's own coordinates are measured against.
pub fn rect_style(style: &RectStyle, rect: Rect, out: &mut String) {
    // One property, because a background is one property: the frame is a layer over the fill, and a fill that is a colour may only be the last of them.
    match (style.fill.as_ref().map(paint), frame_image(style, rect)) {
        (Some(fill), Some(frame)) => declare(out, "background", &format!("{frame},{fill}")),
        (Some(fill), None) => declare(out, "background", &fill),
        (None, Some(frame)) => declare(out, "background", &frame),
        (None, None) => {}
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

/// Whether text in this style takes the element's background for itself.
///
/// A box paints its own background, so it cannot also be the element this text is drawn on: what makes the glyphs a gradient is a background clipped to their shape, and the box's fill would be clipped to it too.
pub fn text_claims_background(style: &TextStyle) -> bool {
    matches!(style.color, Paint::Gradient(_))
}

/// What a `Text` command contributes to the element that holds it.
pub fn text_style(style: &TextStyle, out: &mut String) {
    declare(out, "font-size", &px(style.font_size));
    match &style.color {
        Paint::Solid(ink) => declare(out, "color", &color(*ink)),
        // `color` takes a colour, so a gradient written there was dropped and the text came out in whatever it had inherited. Painted behind the element and clipped to the glyphs instead, the only way a document fills text with anything but a colour. It costs the element its own background, which is why a text asking for one is never folded into the box it is in.
        Paint::Gradient(g) => {
            declare(out, "background-image", &gradient(g));
            declare(out, "-webkit-background-clip", "text");
            declare(out, "background-clip", "text");
            declare(out, "color", "transparent");
        }
    }
    if let Some(cast) = style.text_shadow.cast() {
        // No spread: `text-shadow` has no such length, and the glyphs are not a shape to grow.
        declare(
            out,
            "text-shadow",
            &format!(
                "{} {} {} {}",
                px(cast.offset_x),
                px(cast.offset_y),
                px(cast.blur_radius),
                color(cast.color)
            ),
        );
    }
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
    // Always said, because the document's default is not Telar's. `normal` collapses a run of spaces to one and a newline to a space, so a source listing came out as a single line thousands of characters wide. What Telar means is `pre-wrap`. It is also what keeps the two engines agreeing on height: a document that collapsed newlines measured one line where layout had reserved twenty, and the scroll area went on believing there was content the browser no longer had.
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
#[path = "paint_test.rs"]
mod tests;

#[cfg(test)]
#[path = "paint_text_test.rs"]
mod text_tests;

/// An absolute-space matrix as the CSS transform of a box whose own top-left is at `(x, y)`.
///
/// Telar's matrices move points in the surface's coordinates: a rotation about a box's centre carries that centre, measured from the corner of the page. A CSS transform moves an element within its own coordinates instead, so the same six numbers written out unchanged displace it by its whole distance from that corner — which is a spinner orbiting the window rather than turning, and a slider's fill stretched across it.
///
/// Rebasing puts the fixed point back where the widget meant it. Paired with `transform-origin: 0 0`, since the arithmetic assumes the element's own origin is what stays put, and CSS otherwise assumes its centre.
pub fn matrix(m: [f32; 6], x: f32, y: f32) -> String {
    let [a, b, c, d, e, f] = m;
    format!(
        "matrix({},{},{},{},{},{})",
        round(a),
        round(b),
        round(c),
        round(d),
        round(a * x + c * y + e - x),
        round(b * x + d * y + f - y)
    )
}

#[cfg(test)]
#[path = "paint_matrix_test.rs"]
mod matrix_tests;
