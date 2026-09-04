//! Arbitrary drawing, as the SVG that draws it.
//!
//! A box a document can *place* is a box CSS lays out; a shape a document can only *draw* is an SVG. Every
//! primitive Telar has that is not a box arrives here — artwork, a bitmap, whatever an immediate-mode canvas
//! put on the surface — in the coordinates of the element that holds it, which is exactly what an `<svg>`
//! with no `viewBox` reads its children in.
//!
//! Built as one string and handed over whole. The alternative — reconciling shape elements one by one, as
//! the boxes are — would be paying for identity that nothing here has: a canvas rebuilds its artwork from
//! scratch every time it is asked, so there is no shape to move, only a picture to replace.

use geometry_core::{Point, Rect};
use renderer_core::{
    BorderRadius, Gradient, GradientKind, Paint, PathData, PathStyle, PathVerb, Raster, RectStyle,
    Stroke, TextStyle,
};

use crate::paint::{color, round};

/// The content of one drawing element, as markup.
pub struct Drawing {
    defs: String,
    body: String,
    /// Groups opened and not yet closed, so a stray `Pop` cannot close the document.
    depth: usize,
    next_def: u32,
    /// Namespaces this element's definition ids, since every drawing in the page shares one id space.
    prefix: u64,
}

impl Drawing {
    pub fn new(prefix: u64) -> Self {
        Self {
            defs: String::new(),
            body: String::new(),
            depth: 0,
            next_def: 0,
            prefix,
        }
    }

    pub fn rect(&mut self, rect: Rect, style: &RectStyle) {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        let filter = style.shadow.map(|s| self.drop_shadow(s));
        let mut attrs = String::new();
        if let Some(fill) = &style.fill {
            let value = self.paint(fill);
            attr(&mut attrs, "fill", &value);
        } else {
            attr(&mut attrs, "fill", "none");
        }
        if let Some(filter) = filter {
            attr(&mut attrs, "filter", &format!("url(#{filter})"));
        }
        // A border sits inside the box, where a stroke straddles the edge it is given: half of it is drawn
        // outside, so the shape it is drawn on is the box already pulled in by that half.
        let inset = match &style.border {
            Some(border) if border.is_visible() => {
                let width = border.widths[0];
                let value = self.paint(&border.paint);
                attr(&mut attrs, "stroke", &value);
                attr(&mut attrs, "stroke-width", &round(width));
                width / 2.0
            }
            _ => 0.0,
        };
        let shape = Rect::new(
            rect.x + inset,
            rect.y + inset,
            (rect.width - inset * 2.0).max(0.0),
            (rect.height - inset * 2.0).max(0.0),
        );
        match uniform(style.radius) {
            Some(r) => {
                let mut out = format!(
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"",
                    round(shape.x),
                    round(shape.y),
                    round(shape.width),
                    round(shape.height)
                );
                if r > 0.0 {
                    out.push_str(&format!(" rx=\"{}\"", round(r.min(shape.width / 2.0))));
                }
                out.push_str(&attrs);
                out.push_str("/>");
                self.body.push_str(&out);
            }
            // Four different corners is not a shape `<rect>` can be: SVG carries one radius, so the outline
            // is drawn instead.
            None => {
                self.body.push_str(&format!(
                    "<path d=\"{}\"{attrs}/>",
                    rounded_outline(shape, style.radius)
                ));
            }
        }
    }

    pub fn text(&mut self, text: &str, rect: Rect, style: &TextStyle) {
        if text.is_empty() {
            return;
        }
        // Laid out by the browser rather than placed as an SVG `<text>`: wrapping, alignment, line height and
        // the clamp are all things a Telar text style can ask for and an SVG text run cannot do, and the CSS
        // that answers them is the same CSS a text in a box already gets.
        let mut css = String::new();
        crate::paint::text_style(style, &mut css);
        self.body.push_str(&format!(
            "<foreignObject x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"><div xmlns=\"http://www.w3.org/1999/xhtml\" style=\"{}\">{}</div></foreignObject>",
            round(rect.x),
            round(rect.y),
            round(rect.width.max(0.0)),
            round(rect.height.max(0.0)),
            escape(&css),
            escape(text),
        ));
    }

    pub fn path(&mut self, data: &PathData, style: &PathStyle) {
        let d = outline(data);
        if d.is_empty() {
            return;
        }
        let filter = style.shadow.map(|s| self.drop_shadow(s));
        let mut attrs = String::new();
        match &style.fill {
            Some(fill) => {
                let value = self.paint(fill);
                attr(&mut attrs, "fill", &value);
            }
            None => attr(&mut attrs, "fill", "none"),
        }
        if style.fill_rule == renderer_core::FillRule::EvenOdd {
            attr(&mut attrs, "fill-rule", "evenodd");
        }
        if let Some(stroke) = &style.stroke {
            self.stroke(&mut attrs, stroke);
        }
        if let Some(filter) = filter {
            attr(&mut attrs, "filter", &format!("url(#{filter})"));
        }
        self.body.push_str(&format!("<path d=\"{d}\"{attrs}/>"));
    }

    pub fn line(&mut self, p1: Point, p2: Point, style: &Stroke) {
        let mut attrs = String::new();
        self.stroke(&mut attrs, style);
        self.body.push_str(&format!(
            "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"{attrs}/>",
            round(p1.x),
            round(p1.y),
            round(p2.x),
            round(p2.y)
        ));
    }

    /// A bitmap already resolved to something the document can load — `object-fit` was applied when `rect`
    /// was computed, so the picture is stretched to exactly it.
    pub fn image(&mut self, href: &str, rect: Rect, raster: Raster) {
        let rendering = match raster {
            Raster::Pixel => " style=\"image-rendering:pixelated\"",
            Raster::Smooth => "",
        };
        self.body.push_str(&format!(
            "<image x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" preserveAspectRatio=\"none\"{rendering} href=\"{}\"/>",
            round(rect.x),
            round(rect.y),
            round(rect.width.max(0.0)),
            round(rect.height.max(0.0)),
            escape(href),
        ));
    }

    pub fn open_clip(&mut self, rect: Rect, radius: BorderRadius) {
        let id = self.def_id();
        let shape = match uniform(radius) {
            Some(r) if r > 0.0 => format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\"/>",
                round(rect.x),
                round(rect.y),
                round(rect.width.max(0.0)),
                round(rect.height.max(0.0)),
                round(r)
            ),
            Some(_) => format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>",
                round(rect.x),
                round(rect.y),
                round(rect.width.max(0.0)),
                round(rect.height.max(0.0))
            ),
            None => format!("<path d=\"{}\"/>", rounded_outline(rect, radius)),
        };
        self.defs
            .push_str(&format!("<clipPath id=\"{id}\">{shape}</clipPath>"));
        self.open(&format!("<g clip-path=\"url(#{id})\">"));
    }

    pub fn open_matrix(&mut self, m: [f32; 6]) {
        let [a, b, c, d, e, f] = m;
        self.open(&format!(
            "<g transform=\"matrix({},{},{},{},{},{})\">",
            round(a),
            round(b),
            round(c),
            round(d),
            round(e),
            round(f)
        ));
    }

    pub fn open_layer(&mut self, opacity: f32) {
        self.open(&format!("<g opacity=\"{}\">", round(opacity)));
    }

    pub fn close_group(&mut self) {
        if self.depth == 0 {
            return;
        }
        self.depth -= 1;
        self.body.push_str("</g>");
    }

    /// The markup for this element's children, with everything still open closed.
    pub fn finish(mut self) -> String {
        while self.depth > 0 {
            self.close_group();
        }
        if self.defs.is_empty() {
            return self.body;
        }
        format!("<defs>{}</defs>{}", self.defs, self.body)
    }

    fn open(&mut self, tag: &str) {
        self.body.push_str(tag);
        self.depth += 1;
    }

    fn stroke(&mut self, attrs: &mut String, stroke: &Stroke) {
        let value = self.paint(&stroke.paint);
        attr(attrs, "stroke", &value);
        attr(attrs, "stroke-width", &round(stroke.width));
        match stroke.cap {
            renderer_core::LineCap::Butt => {}
            renderer_core::LineCap::Round => attr(attrs, "stroke-linecap", "round"),
            renderer_core::LineCap::Square => attr(attrs, "stroke-linecap", "square"),
        }
        match stroke.join {
            renderer_core::LineJoin::Miter => {}
            renderer_core::LineJoin::Round => attr(attrs, "stroke-linejoin", "round"),
            renderer_core::LineJoin::Bevel => attr(attrs, "stroke-linejoin", "bevel"),
        }
    }

    fn paint(&mut self, p: &Paint) -> String {
        match p {
            Paint::Solid(c) => color(*c),
            Paint::Gradient(g) => {
                let id = self.def_id();
                self.defs.push_str(&gradient_def(&id, g));
                format!("url(#{id})")
            }
        }
    }

    fn drop_shadow(&mut self, s: renderer_core::Shadow) -> String {
        let id = self.def_id();
        // A blur radius is the width of the whole falloff, where a Gaussian is described by its deviation;
        // the same halving CSS does for `box-shadow`.
        self.defs.push_str(&format!(
            "<filter id=\"{id}\" x=\"-50%\" y=\"-50%\" width=\"200%\" height=\"200%\"><feDropShadow dx=\"{}\" dy=\"{}\" stdDeviation=\"{}\" flood-color=\"{}\"/></filter>",
            round(s.offset_x),
            round(s.offset_y),
            round(s.blur_radius / 2.0),
            color(s.color)
        ));
        id
    }

    fn def_id(&mut self) -> String {
        self.next_def += 1;
        format!("t{}-{}", self.prefix, self.next_def)
    }
}

/// A box's gradient frame, as a picture of the ring it is.
///
/// Every other frame here is an inset `box-shadow`, which follows the radius and takes no room from the box
/// — but a shadow carries a colour, and a gradient is not one. So the ring is drawn: the box's own outline
/// with the inner edge punched out of it under the even-odd rule, which is the shape the rasterising
/// backends fill for the same border.
///
/// Drawn in the box's own corner rather than where it stands on the surface, so the picture can be laid over
/// it as a background of exactly its size — and the gradient moves with it, since its points were measured
/// against the rect the widget drew.
pub fn frame_svg(
    rect: Rect,
    radius: BorderRadius,
    widths: [f32; 4],
    gradient: &Gradient,
) -> String {
    let outer = Rect::new(0.0, 0.0, rect.width, rect.height);
    let mut d = rounded_outline(outer, radius);
    // No interior means the frame swallowed the box, and the outer outline alone is the whole of it.
    if let Some((inner, inner_radius)) = renderer_core::border_inner_shape(outer, radius, widths) {
        d.push_str(&rounded_outline(inner, inner_radius));
    }
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\"><defs>{}</defs><path d=\"{d}\" fill=\"url(#f)\" fill-rule=\"evenodd\"/></svg>",
        round(rect.width),
        round(rect.height),
        gradient_def("f", &moved(*gradient, -rect.x, -rect.y)),
    )
}

fn moved(g: Gradient, dx: f32, dy: f32) -> Gradient {
    let kind = match g.kind {
        GradientKind::Linear { start, end } => GradientKind::Linear {
            start: Point::new(start.x + dx, start.y + dy),
            end: Point::new(end.x + dx, end.y + dy),
        },
        GradientKind::Radial { center, radius } => GradientKind::Radial {
            center: Point::new(center.x + dx, center.y + dy),
            radius,
        },
    };
    Gradient { kind, ..g }
}

fn gradient_def(id: &str, g: &Gradient) -> String {
    let stops: String = g
        .stops
        .active()
        .iter()
        .map(|stop| {
            format!(
                "<stop offset=\"{}\" stop-color=\"{}\"/>",
                round(stop.position),
                color(stop.color)
            )
        })
        .collect();
    // User space, not the default object bounding box: the points are the ones the widget drew with, in the
    // same coordinates as the shape they paint.
    match g.kind {
        GradientKind::Linear { start, end } => format!(
            "<linearGradient id=\"{id}\" gradientUnits=\"userSpaceOnUse\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\">{stops}</linearGradient>",
            round(start.x),
            round(start.y),
            round(end.x),
            round(end.y)
        ),
        GradientKind::Radial { center, radius } => format!(
            "<radialGradient id=\"{id}\" gradientUnits=\"userSpaceOnUse\" cx=\"{}\" cy=\"{}\" r=\"{}\">{stops}</radialGradient>",
            round(center.x),
            round(center.y),
            round(radius)
        ),
    }
}

/// The `d` a path's verbs spell.
fn outline(data: &PathData) -> String {
    let mut out = String::new();
    for verb in data.verbs() {
        match verb {
            PathVerb::MoveTo(p) => {
                out.push('M');
                point(&mut out, *p);
            }
            PathVerb::LineTo(p) => {
                out.push('L');
                point(&mut out, *p);
            }
            PathVerb::QuadTo { ctrl, to } => {
                out.push('Q');
                point(&mut out, *ctrl);
                out.push(' ');
                point(&mut out, *to);
            }
            PathVerb::CubicTo { ctrl1, ctrl2, to } => {
                out.push('C');
                point(&mut out, *ctrl1);
                out.push(' ');
                point(&mut out, *ctrl2);
                out.push(' ');
                point(&mut out, *to);
            }
            PathVerb::Close => out.push('Z'),
        }
    }
    out
}

/// A rectangle whose four corners are rounded differently, as a path.
fn rounded_outline(rect: Rect, radius: BorderRadius) -> String {
    let limit = (rect.width.min(rect.height) / 2.0).max(0.0);
    let clamp = |r: f32| r.clamp(0.0, limit);
    let (tl, tr, br, bl) = (
        clamp(radius.top_left),
        clamp(radius.top_right),
        clamp(radius.bottom_right),
        clamp(radius.bottom_left),
    );
    let (x, y, w, h) = (rect.x, rect.y, rect.width, rect.height);
    let arc = |r: f32, to_x: f32, to_y: f32| {
        if r > 0.0 {
            format!(
                "A{} {} 0 0 1 {} {}",
                round(r),
                round(r),
                round(to_x),
                round(to_y)
            )
        } else {
            format!("L{} {}", round(to_x), round(to_y))
        }
    };
    format!(
        "M{} {}L{} {}{}L{} {}{}L{} {}{}L{} {}{}Z",
        round(x + tl),
        round(y),
        round(x + w - tr),
        round(y),
        arc(tr, x + w, y + tr),
        round(x + w),
        round(y + h - br),
        arc(br, x + w - br, y + h),
        round(x + bl),
        round(y + h),
        arc(bl, x, y + h - bl),
        round(x),
        round(y + tl),
        arc(tl, x + tl, y),
    )
}

fn point(out: &mut String, p: Point) {
    out.push_str(&round(p.x));
    out.push(' ');
    out.push_str(&round(p.y));
}

/// The one radius all four corners share, or `None` where they differ.
fn uniform(r: BorderRadius) -> Option<f32> {
    (r.top_left == r.top_right && r.top_left == r.bottom_right && r.top_left == r.bottom_left)
        .then_some(r.top_left)
}

fn attr(out: &mut String, name: &str, value: &str) {
    out.push(' ');
    out.push_str(name);
    out.push_str("=\"");
    out.push_str(&escape(value));
    out.push('"');
}

/// Markup-safe text. Every string that reaches this file is one the application chose — a label, a font
/// family, a path a picture was loaded from — so none of it can be assumed to be markup already.
fn escape(value: &str) -> String {
    if !value.contains(['&', '<', '>', '"', '\'']) {
        return value.to_string();
    }
    let mut out = String::with_capacity(value.len() + 8);
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use renderer_core::{Color, FillRule, LineCap, Shadow, ShapeStyle};

    fn drawing() -> Drawing {
        Drawing::new(7)
    }

    #[test]
    fn an_empty_drawing_has_nothing_to_show() {
        assert_eq!(drawing().finish(), "");
    }

    #[test]
    fn a_path_becomes_its_verbs() {
        let data = PathData::new()
            .move_to(Point::new(0.0, 0.0))
            .line_to(Point::new(10.0, 0.0))
            .quad_to(Point::new(15.0, 5.0), Point::new(10.0, 10.0))
            .cubic_to(
                Point::new(8.0, 12.0),
                Point::new(2.0, 12.0),
                Point::new(0.0, 10.0),
            )
            .close();
        let mut d = drawing();
        d.path(&data, &PathStyle::default().with_fill(Color::BLACK));
        let out = d.finish();
        assert!(
            out.contains("d=\"M0 0L10 0Q15 5 10 10C8 12 2 12 0 10Z\""),
            "{out}"
        );
        assert!(out.contains("fill=\"#000000\""), "{out}");
    }

    #[test]
    fn a_path_with_no_fill_is_not_filled_black() {
        let data = PathData::new()
            .move_to(Point::new(0.0, 0.0))
            .line_to(Point::new(10.0, 10.0));
        let mut d = drawing();
        d.path(
            &data,
            &PathStyle::default()
                .with_stroke(Stroke::new(Color::BLACK, 2.0).with_cap(LineCap::Round)),
        );
        let out = d.finish();
        assert!(out.contains("fill=\"none\""), "{out}");
        assert!(out.contains("stroke-width=\"2\""), "{out}");
        assert!(out.contains("stroke-linecap=\"round\""), "{out}");
    }

    #[test]
    fn an_even_odd_fill_says_so() {
        let data = PathData::new()
            .move_to(Point::new(0.0, 0.0))
            .line_to(Point::new(4.0, 0.0))
            .close();
        let mut d = drawing();
        d.path(
            &data,
            &PathStyle::default()
                .with_fill(Color::WHITE)
                .with_fill_rule(FillRule::EvenOdd),
        );
        assert!(d.finish().contains("fill-rule=\"evenodd\""));
    }

    #[test]
    fn a_gradient_becomes_a_definition_the_shape_points_at() {
        let mut d = drawing();
        d.path(
            &PathData::new()
                .move_to(Point::new(0.0, 0.0))
                .line_to(Point::new(4.0, 4.0)),
            &PathStyle::default().with_fill(Paint::Gradient(Gradient::linear(
                Point::new(0.0, 0.0),
                Point::new(0.0, 20.0),
                &[(0.0, Color::BLACK), (1.0, Color::WHITE)],
            ))),
        );
        let out = d.finish();
        assert!(
            out.starts_with("<defs><linearGradient id=\"t7-1\""),
            "{out}"
        );
        assert!(out.contains("gradientUnits=\"userSpaceOnUse\""), "{out}");
        assert!(out.contains("y2=\"20\""), "{out}");
        assert!(out.contains("fill=\"url(#t7-1)\""), "{out}");
    }

    #[test]
    fn a_uniform_radius_is_a_rect_and_a_mixed_one_is_a_path() {
        let mut d = drawing();
        d.rect(
            Rect::new(0.0, 0.0, 20.0, 10.0),
            &RectStyle::default()
                .with_fill(Color::BLACK)
                .with_radius(BorderRadius::all(3.0)),
        );
        assert!(
            d.finish()
                .contains("<rect x=\"0\" y=\"0\" width=\"20\" height=\"10\" rx=\"3\"")
        );

        let mut d = drawing();
        d.rect(
            Rect::new(0.0, 0.0, 20.0, 10.0),
            &RectStyle::default()
                .with_fill(Color::BLACK)
                .with_radius(BorderRadius {
                    top_left: 4.0,
                    top_right: 0.0,
                    bottom_right: 4.0,
                    bottom_left: 0.0,
                }),
        );
        let out = d.finish();
        assert!(out.starts_with("<path d=\"M4 0"), "{out}");
        assert!(out.contains("A4 4 0 0 1"), "{out}");
    }

    #[test]
    fn a_border_is_drawn_inside_the_box_a_stroke_would_straddle() {
        let mut d = drawing();
        d.rect(
            Rect::new(0.0, 0.0, 20.0, 20.0),
            &RectStyle::default().with_border(renderer_core::Border::uniform(Color::BLACK, 4.0)),
        );
        let out = d.finish();
        assert!(
            out.contains("x=\"2\" y=\"2\" width=\"16\" height=\"16\""),
            "{out}"
        );
        assert!(out.contains("stroke-width=\"4\""), "{out}");
    }

    #[test]
    fn a_shadow_becomes_a_filter() {
        let mut d = drawing();
        d.rect(
            Rect::new(0.0, 0.0, 10.0, 10.0),
            &RectStyle::default()
                .with_fill(Color::BLACK)
                .with_shadow(Shadow {
                    offset_x: 0.0,
                    offset_y: 2.0,
                    blur_radius: 8.0,
                    spread: 0.0,
                    color: Color::BLACK,
                }),
        );
        let out = d.finish();
        assert!(
            out.contains("<feDropShadow dx=\"0\" dy=\"2\" stdDeviation=\"4\""),
            "{out}"
        );
        assert!(out.contains("filter=\"url(#t7-1)\""), "{out}");
    }

    #[test]
    fn text_carries_the_css_a_box_would_have_given_it() {
        let mut d = drawing();
        d.text(
            "fill & stroke",
            Rect::new(2.0, 4.0, 100.0, 14.0),
            &TextStyle::new(11.0, Color::BLACK),
        );
        let out = d.finish();
        assert!(
            out.contains("<foreignObject x=\"2\" y=\"4\" width=\"100\" height=\"14\">"),
            "{out}"
        );
        assert!(
            out.contains("xmlns=\"http://www.w3.org/1999/xhtml\""),
            "{out}"
        );
        assert!(out.contains("font-size:11px"), "{out}");
        assert!(out.contains(">fill &amp; stroke<"), "{out}");
    }

    #[test]
    fn groups_close_in_the_order_they_opened() {
        let mut d = drawing();
        d.open_layer(0.5);
        d.open_matrix([2.0, 0.0, 0.0, 2.0, 4.0, 4.0]);
        d.line(
            Point::new(0.0, 0.0),
            Point::new(4.0, 4.0),
            &Stroke::new(Color::BLACK, 1.0),
        );
        d.close_group();
        let out = d.finish();
        assert_eq!(
            out,
            "<g opacity=\"0.5\"><g transform=\"matrix(2,0,0,2,4,4)\"><line x1=\"0\" y1=\"0\" x2=\"4\" y2=\"4\" stroke=\"#000000\" stroke-width=\"1\"/></g></g>"
        );
    }

    #[test]
    fn a_pop_with_nothing_open_closes_nothing() {
        let mut d = drawing();
        d.close_group();
        d.line(
            Point::new(0.0, 0.0),
            Point::new(1.0, 1.0),
            &Stroke::new(Color::BLACK, 1.0),
        );
        assert!(!d.finish().contains("</g>"));
    }

    #[test]
    fn a_clip_is_a_definition_the_group_points_at() {
        let mut d = drawing();
        d.open_clip(Rect::new(0.0, 0.0, 30.0, 30.0), BorderRadius::all(6.0));
        d.image(
            "data:image/png;base64,AAA",
            Rect::new(0.0, 0.0, 30.0, 30.0),
            Raster::Pixel,
        );
        let out = d.finish();
        assert!(out.contains("<clipPath id=\"t7-1\"><rect x=\"0\" y=\"0\" width=\"30\" height=\"30\" rx=\"6\"/></clipPath>"), "{out}");
        assert!(out.contains("<g clip-path=\"url(#t7-1)\">"), "{out}");
        assert!(out.contains("image-rendering:pixelated"), "{out}");
        assert!(out.ends_with("</g>"), "{out}");
    }

    #[test]
    fn a_collapsed_rect_draws_nothing() {
        let mut d = drawing();
        d.rect(
            Rect::new(0.0, 0.0, 0.0, 10.0),
            &RectStyle::default().with_fill(Color::BLACK),
        );
        assert_eq!(d.finish(), "");
    }

    /// The ring the rasterising backends fill, as a picture that can be laid over the box: outer outline,
    /// inner outline, even-odd. Drawn in the box's own corner, and the gradient moved to meet it — measured
    /// where the widget drew it, the colours would have started off the left edge of the picture.
    #[test]
    fn a_gradient_frame_is_a_ring_drawn_in_the_box_s_own_corner() {
        let svg = frame_svg(
            Rect::new(300.0, 500.0, 100.0, 40.0),
            BorderRadius::all(8.0),
            [2.0; 4],
            &Gradient::linear(
                Point::new(300.0, 500.0),
                Point::new(400.0, 500.0),
                &[(0.0, Color::BLACK), (1.0, Color::WHITE)],
            ),
        );
        assert!(svg.contains("width=\"100\" height=\"40\""), "{svg}");
        assert!(svg.contains("fill-rule=\"evenodd\""), "{svg}");
        assert!(
            svg.contains("x1=\"0\" y1=\"0\" x2=\"100\" y2=\"0\""),
            "{svg}"
        );
        assert!(svg.contains("d=\"M8 0"), "{svg}");
        assert!(svg.contains("M8 2"), "{svg}");
        assert!(svg.contains("A6 6 0 0 1"), "{svg}");
    }

    /// A frame thicker than the box it frames has no interior to punch out, and the outline alone is the
    /// whole of it — a second subpath of nothing would have cut a hole under the even-odd rule.
    #[test]
    fn a_frame_that_swallows_its_box_is_the_outline_alone() {
        let svg = frame_svg(
            Rect::new(0.0, 0.0, 6.0, 6.0),
            BorderRadius::zero(),
            [4.0; 4],
            &Gradient::linear(
                Point::new(0.0, 0.0),
                Point::new(6.0, 0.0),
                &[(0.0, Color::BLACK), (1.0, Color::WHITE)],
            ),
        );
        assert_eq!(svg.matches('M').count(), 1, "{svg}");
    }
}
