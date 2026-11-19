//! A box's painted style, as the CSS that draws it.
//!
//! Separate from the layout half on purpose: layout comes from what the widget declared, paint comes from the `Rect` the widget drew. The two reach a document backend by different routes and are only joined here, in the string one element ends up with.

use geometry_core::{Point, Rect, Transform};
use renderer_core::{
    Border, BorderKind, BorderRadius, Color, Gradient, GradientStops, Paint, RectStyle,
    ResolvedGradient, Shadow, TextStyle,
};

/// The custom property a forced-colors document reads a frame's colour from: set on the frames that mark where the keyboard is, so the sheet can give them the highlight instead of the text colour.
pub(crate) const RING_PROPERTY: &str = "--telar-ring";

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

/// `value` CSS pixels as `rem` of a root of `root_font_size` pixels, to a hundred-thousandth so a size of a sixteenth of a pixel comes back whole at the default root.
pub fn rem(value: f32, root_font_size: f32) -> String {
    let rem = ((value / root_font_size) * 100_000.0).round() / 100_000.0;
    format!("{rem}rem")
}

/// A paint as the CSS value a `background` or `color` takes.
pub fn paint(p: &Paint) -> String {
    match p {
        Paint::Solid(c) => color(*c),
        Paint::Gradient(g) => gradient(g),
    }
}

fn gradient(g: &Gradient) -> String {
    let stops_css = |stops: &GradientStops| {
        stops
            .active()
            .iter()
            .map(|stop| format!("{} {}%", color(stop.color), round(stop.position * 100.0)))
            .collect::<Vec<_>>()
            .join(",")
    };
    match g.resolve() {
        ResolvedGradient::Solid(c) => color(c),
        // CSS measures the angle clockwise from up, where the vector is measured from the positive x axis with y growing downwards — hence the quarter turn and the sign.
        ResolvedGradient::Linear {
            start, end, stops, ..
        } => {
            let (dx, dy) = (end.x - start.x, end.y - start.y);
            let degrees = dx.atan2(-dy).to_degrees().rem_euclid(360.0);
            format!(
                "linear-gradient({}deg,{})",
                round(degrees),
                stops_css(&stops)
            )
        }
        // CSS cannot turn a radial gradient, so a tilted ellipse becomes the upright one that fills the same bounding box.
        ResolvedGradient::Radial {
            transform: t,
            stops,
            ..
        } => format!(
            "radial-gradient({} {} at center,{})",
            px(t.a.hypot(t.c)),
            px(t.b.hypot(t.d)),
            stops_css(&stops)
        ),
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

/// A box's frame, as the real `outline` or `border` that draws it, so a document in forced colours paints it too: a shadow is dropped there, a frame is recoloured.
///
/// An even frame is an `outline` pulled that far inward, which follows the radius and costs the layout nothing: a CSS border eats into the content box, and a 1px frame left every child two pixels narrower than the rect hit-testing reads. A frame uneven on its sides has no outline to be, so it is a `border` of those widths, which takes the room they come to out of the box it is on: the box's own element never wears one (see [`own_box_style`]).
///
/// A gradient is not a colour a frame can take, so it is drawn as a background layer (see [`frame_image`]) and the frame itself is left transparent, which forced colours still has something to repaint.
fn border(b: &Border, out: &mut String) {
    if !b.is_visible() {
        return;
    }
    let colour = match b.paint {
        Paint::Solid(colour) => color(colour),
        Paint::Gradient(_) => color(Color::TRANSPARENT),
    };
    if b.kind == BorderKind::FocusRing {
        declare(out, RING_PROPERTY, "Highlight");
    }
    let [top, right, bottom, left] = b.widths;
    if is_even(b) {
        declare(out, "outline", &format!("{} solid {colour}", px(top)));
        declare(out, "outline-offset", &px(-top));
        return;
    }
    declare(out, "border-style", "solid");
    declare(out, "border-color", &colour);
    declare(
        out,
        "border-width",
        &format!("{} {} {} {}", px(top), px(right), px(bottom), px(left)),
    );
}

/// Whether a frame is the same on every side, which is the one a box can draw as an outline.
fn is_even(b: &Border) -> bool {
    let [top, right, bottom, left] = b.widths;
    top == right && top == bottom && top == left
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

/// A gradient frame, as the background layer that draws it — `None` for a solid frame, which [`border`] draws itself.
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
    declarations(style, rect, out, true);
}

/// [`rect_style`] for the box's own element, where a frame uneven on its sides is not drawn on it: a CSS border takes its width out of the content box, which layout never reserved. It comes back as the style of a piece laid over the box instead, which is the same frame without the cost.
pub fn own_box_style(style: &RectStyle, rect: Rect, out: &mut String) -> Option<String> {
    let uneven = style
        .border
        .as_ref()
        .filter(|b| b.is_visible() && !is_even(b));
    declarations(style, rect, out, uneven.is_none());
    let frame = uneven?;
    let mut css = String::new();
    if let Some(value) = radius(style.radius) {
        declare(&mut css, "border-radius", &value);
    }
    border(frame, &mut css);
    Some(css)
}

fn declarations(style: &RectStyle, rect: Rect, out: &mut String, framed: bool) {
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
    if let Some(frame) = style.border.as_ref().filter(|_| framed) {
        border(frame, out);
    }
    if let Some(drop) = style.shadow {
        declare(out, "box-shadow", &shadow(drop));
    }
}

/// Whether text in this style takes the element's background for itself.
///
/// A box paints its own background, so it cannot also be the element this text is drawn on: what makes the glyphs a gradient is a background clipped to their shape, and the box's fill would be clipped to it too.
pub fn text_claims_background(style: &TextStyle) -> bool {
    matches!(style.color, Paint::Gradient(_))
}

/// The CSS `font-family` value for `family`, or `None` for the bare default — nobody named anything, so nothing is declared and the document's own stylesheet decides, exactly as before there was a generic to name.
pub fn font_family_list(family: &renderer_core::FontFamily) -> Option<String> {
    use renderer_core::FontFamily;
    let mut pieces = Vec::new();
    match family {
        FontFamily::SansSerif => return None,
        // A bare name with no author-written fallback still degrades to the platform's sans-serif, as it always has.
        FontFamily::Named(name) => {
            pieces.push(renderer_core::css_string(name));
            pieces.push("sans-serif".to_string());
        }
        other => push_family_piece(other, &mut pieces),
    }
    Some(pieces.join(","))
}

/// One member of a `Stack`, or the sole family when there is no stack at all — every generic goes out as its CSS keyword, unquoted, which is the fix: quoting `monospace` is what kept the generic from ever applying.
fn push_family_piece(family: &renderer_core::FontFamily, out: &mut Vec<String>) {
    use renderer_core::FontFamily;
    match family {
        FontFamily::SansSerif => out.push("sans-serif".to_string()),
        FontFamily::Serif => out.push("serif".to_string()),
        FontFamily::Monospace => out.push("monospace".to_string()),
        FontFamily::SystemUi => out.push("system-ui".to_string()),
        FontFamily::Cursive => out.push("cursive".to_string()),
        FontFamily::Fantasy => out.push("fantasy".to_string()),
        FontFamily::Named(name) => out.push(renderer_core::css_string(name)),
        FontFamily::Stack(families) => {
            for member in families.iter() {
                push_family_piece(member, out);
            }
        }
    }
}

/// What a `Text` command contributes to the element that holds it, its size in `rem` of a root of `root_font_size` CSS pixels.
///
/// In `rem` so a change of the reader's text size reaches the text the moment the browser applies it, and so does the size they had set before the page loaded, on a page written ahead of time at the browser's default root. Layout has already scaled the size by the text scale that root stands for, so at that root the text is drawn at the size it was measured at.
pub fn text_style(style: &TextStyle, root_font_size: f32, out: &mut String) {
    declare(out, "font-size", &rem(style.font_size, root_font_size));
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
    if let Some(family) = font_family_list(&style.font_family) {
        declare(out, "font-family", &family);
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
    if !style.font_variations.is_empty() {
        declare(
            out,
            "font-variation-settings",
            &style.font_variations.to_css(),
        );
    }
    if !style.font_features.is_empty() {
        declare(out, "font-feature-settings", &style.font_features.to_css());
    }
    if style.text_case != renderer_core::TextCase::AsWritten {
        declare(out, "text-transform", style.text_case.css_name());
    }
    decoration_style(&style.decoration, &style.color, out);
    // Always `pre-wrap`, because the document's default `normal` collapses spaces and newlines, which would turn a source listing into one line and leave the browser measuring one line where layout reserved twenty.
    declare(
        out,
        "white-space",
        if style.text_wrap == renderer_core::TextWrap::NoWrap {
            "pre"
        } else {
            "pre-wrap"
        },
    );
    if style.text_wrap == renderer_core::TextWrap::Wrap {
        // `break-word` rather than `anywhere`: both break a word wider than the box between grapheme clusters as the measurers do, but `anywhere` also shrinks the min-content width to one cluster where the measurers keep the widest word.
        declare(out, "overflow-wrap", "break-word");
    }
    if let Some(max) = style.clamp.map(|clamp| clamp.max_lines()) {
        // The only cross-browser line clamp there is, and it needs all four declarations to work.
        declare(out, "display", "-webkit-box");
        declare(out, "-webkit-box-orient", "vertical");
        declare(out, "-webkit-line-clamp", &max.to_string());
        declare(out, "overflow", "hidden");
    }
}

/// A text length as CSS writes it inside the paragraph: `em` is the paragraph's, as it is to Telar. A fraction of the surface reaches a renderer already in pixels.
fn length(length: renderer_core::TextLength) -> String {
    match length {
        renderer_core::TextLength::Em(em) => format!("{}em", round(em)),
        other => px(other.resolve(0.0, geometry_core::Size::ZERO)),
    }
}

/// What a reading's `<rt>` declares: the size `fit` set it in, in `em` of its base, which is set in `base_size`; one line box no taller than that size, so the room it takes over its base is its size and not the face's own leading; and the tightening its letters close up by.
pub fn reading_style(fit: &renderer_core::RubyFit, base_size: f32, out: &mut String) {
    if base_size > 0.0 {
        declare(
            out,
            "font-size",
            &format!("{}em", round(fit.size / base_size)),
        );
    }
    declare(out, "line-height", "1");
    if fit.tightening > 0.0 {
        declare(out, "letter-spacing", &px(-fit.tightening));
    }
}

/// The declarations a run of a paragraph adds over the paragraph: only what its span says, since everything else it inherits from the element around it. A size in pixels is in `rem` of a root of `root_font_size`, as [`text_style`] writes the paragraph's.
pub fn span_style(over: &renderer_core::Declared, root_font_size: f32, out: &mut String) {
    match over.font_size {
        Some(renderer_core::TextLength::Em(em)) => {
            declare(out, "font-size", &format!("{}em", round(em)))
        }
        Some(size) => declare(
            out,
            "font-size",
            &rem(size.resolve(0.0, geometry_core::Size::ZERO), root_font_size),
        ),
        None => {}
    }
    match &over.color {
        Some(Paint::Solid(ink)) => declare(out, "color", &color(*ink)),
        Some(Paint::Gradient(g)) => {
            declare(out, "background-image", &gradient(g));
            declare(out, "-webkit-background-clip", "text");
            declare(out, "background-clip", "text");
            declare(out, "color", "transparent");
        }
        None => {}
    }
    if let Some(weight) = over.font_weight {
        declare(out, "font-weight", &weight.to_string());
    }
    if let Some(font_style) = over.font_style {
        let slant = if font_style == renderer_core::FontStyle::Normal {
            "normal"
        } else {
            "italic"
        };
        declare(out, "font-style", slant);
    }
    if let Some(family) = over.font_family.as_ref().and_then(font_family_list) {
        declare(out, "font-family", &family);
    }
    if let Some(spacing) = over.letter_spacing {
        declare(out, "letter-spacing", &length(spacing));
    }
    if let Some(axes) = &over.font_variations {
        declare(out, "font-variation-settings", &axes.to_css());
    }
    if let Some(features) = &over.font_features {
        declare(out, "font-feature-settings", &features.to_css());
    }
    if let Some(case) = over.text_case {
        declare(out, "text-transform", case.css_name());
    }
    match over.decoration_line {
        Some(renderer_core::DecorationLine::Underline) => {
            declare(out, "text-decoration-line", "underline");
            declare(out, "text-decoration-skip-ink", "none");
        }
        Some(renderer_core::DecorationLine::None) => declare(out, "text-decoration-line", "none"),
        None => {}
    }
    if let Some(renderer_core::DecorationLength::Length(offset)) = over.decoration_offset {
        declare(out, "text-underline-offset", &length(offset));
    }
    if let Some(renderer_core::DecorationLength::Length(thickness)) = over.decoration_thickness {
        declare(out, "text-decoration-thickness", &length(thickness));
    }
    if let Some(paint) = over.decoration_color {
        declare(out, "text-decoration-color", &color(paint.solid_color()));
    }
}

/// A backdrop blur of Telar's `radius` as the CSS `blur()` that draws it: CSS takes the Gaussian's deviation, half the radius every other target blurs by, as `box-shadow` halves its own.
pub fn backdrop_filter(radius: f32) -> String {
    format!("blur({})", px(renderer_core::blur_sigma(radius)))
}

/// The line a text draws along its glyphs, as the `text-decoration` longhands. Skipping ink is off because no other target lifts the line around a descender, and a gradient comes down to its first stop because CSS paints a decoration in a colour only; text filled with one is `transparent`, so its line is never left to `currentColor`.
fn decoration_style(decoration: &renderer_core::TextDecoration, ink: &Paint, out: &mut String) {
    if !decoration.is_drawn() {
        return;
    }
    declare(out, "text-decoration-line", "underline");
    declare(out, "text-decoration-skip-ink", "none");
    if let renderer_core::DecorationMetric::Px(offset) = decoration.offset {
        declare(out, "text-underline-offset", &px(offset));
    }
    if let renderer_core::DecorationMetric::Px(thickness) = decoration.thickness {
        declare(out, "text-decoration-thickness", &px(thickness));
    }
    let paint = decoration.color.unwrap_or(*ink);
    if decoration.color.is_some() || matches!(paint, Paint::Gradient(_)) {
        declare(out, "text-decoration-color", &color(paint.solid_color()));
    }
}

#[cfg(test)]
#[path = "paint.test.rs"]
mod tests;

#[cfg(test)]
#[path = "paint.text.test.rs"]
mod text_tests;

/// An absolute-space matrix as the CSS transform of a box whose own top-left is at `(x, y)`.
///
/// Telar's matrices move points in the surface's coordinates: a rotation about a box's centre carries that centre, measured from the corner of the page. A CSS transform moves an element within its own coordinates instead, so the same six numbers written out unchanged displace it by its whole distance from that corner — which is a spinner orbiting the window rather than turning, and a slider's fill stretched across it.
///
/// Rebasing puts the fixed point back where the widget meant it. Paired with `transform-origin: 0 0`, since the arithmetic assumes the element's own origin is what stays put, and CSS otherwise assumes its centre.
pub fn matrix(m: Transform, x: f32, y: f32) -> String {
    spelled(rebased(m, x, y))
}

/// An absolute-space matrix in the coordinates of a box whose own top-left is at `(x, y)`: see [`matrix`].
pub fn rebased(m: Transform, x: f32, y: f32) -> Transform {
    let corner = m.apply(Point::new(x, y));
    Transform {
        e: corner.x - x,
        f: corner.y - y,
        ..m
    }
}

/// A matrix as the `matrix()` function CSS and SVG both read.
pub fn spelled(m: Transform) -> String {
    let [a, b, c, d, e, f] = m.to_array().map(round);
    format!("matrix({a},{b},{c},{d},{e},{f})")
}

#[cfg(test)]
#[path = "paint.matrix.test.rs"]
mod matrix_tests;
