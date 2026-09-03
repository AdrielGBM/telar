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

fn border(b: &Border) -> Option<String> {
    if !b.is_visible() {
        return None;
    }
    let [top, right, bottom, left] = b.widths;
    let colour = paint(&b.paint);
    // A gradient border needs `border-image`, which cannot be combined with a radius; a solid colour is the
    // only case a plain border draws faithfully, and the rest fall back to the colour a solid would be.
    let widths = if top == right && top == bottom && top == left {
        px(top)
    } else {
        format!("{} {} {} {}", px(top), px(right), px(bottom), px(left))
    };
    Some(format!("{widths} solid {colour}"))
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
    if let Some(b) = &style.border {
        if let Some(value) = border(b) {
            declare(out, "border", &value);
            // Telar's border sits inside the box, as CSS's does only under `border-box`.
            declare(out, "box-sizing", "border-box");
        }
    }
    if let Some(value) = radius(style.radius) {
        declare(out, "border-radius", &value);
    }
    if let Some(s) = style.shadow {
        declare(out, "box-shadow", &shadow(s));
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
    if style.text_wrap == renderer_core::TextWrap::NoWrap {
        declare(out, "white-space", "nowrap");
    }
    if let Some(max) = style.clamp.max_lines() {
        // The only cross-browser line clamp there is, and it needs all four declarations to work.
        declare(out, "display", "-webkit-box");
        declare(out, "-webkit-box-orient", "vertical");
        declare(out, "-webkit-line-clamp", &max.to_string());
        declare(out, "overflow", "hidden");
    }
}
