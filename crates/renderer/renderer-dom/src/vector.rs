//! Arbitrary drawing, as the SVG that draws it.
//!
//! A box a document can *place* is a box CSS lays out; a shape a document can only *draw* is an SVG. Every primitive Telar has that is not a box arrives here — artwork, a bitmap, whatever an immediate-mode canvas put on the surface — in the coordinates of the element that holds it, which is exactly what an `<svg>` with no `viewBox` reads its children in.
//!
//! Built as one string and handed over whole. The alternative — reconciling shape elements one by one, as the boxes are — would be paying for identity that nothing here has: a canvas rebuilds its artwork from scratch every time it is asked, so there is no shape to move, only a picture to replace.

use geometry_core::{Point, Rect};
use renderer_core::{
    BlendMode, BorderRadius, Gradient, GradientKind, ImageFill, LayerMask, Paint, PathData,
    PathStyle, PathVerb, Raster, RectStyle, Stroke, TextStyle,
};

use crate::paint::{color, round};

/// A group open in the markup.
enum Group {
    Plain,
    /// A box inside the drawing, placed where layout put it relative to the box around it.
    Placed,
    /// A mask source: its markup goes into a `<mask>`, and `drawn` is the picture it was taken out of.
    MaskSource {
        drawn: String,
        id: String,
    },
}

/// The content of one drawing element, as markup.
pub struct Drawing {
    defs: String,
    body: String,
    /// Groups opened and not yet closed, so a stray `Pop` cannot close the document.
    groups: Vec<Group>,
    /// The id of the mask a source just defined, for the layer it applies to.
    pending_mask: Option<String>,
    /// Where each box open inside the drawing lies in layout, innermost last, the drawing's own box first.
    origins: Vec<(f32, f32)>,
    next_def: u32,
    /// Namespaces this element's definition ids, since every drawing in the page shares one id space.
    prefix: u64,
}

impl Drawing {
    pub fn new(prefix: u64) -> Self {
        Self {
            defs: String::new(),
            body: String::new(),
            groups: Vec::new(),
            pending_mask: None,
            origins: vec![(0.0, 0.0)],
            next_def: 0,
            prefix,
        }
    }

    /// A drawing of the box laid out at `origin`, whose boxes inside it are placed relative to it: see [`open_box`](Self::open_box).
    pub fn at(prefix: u64, origin: (f32, f32)) -> Self {
        Self {
            origins: vec![origin],
            ..Self::new(prefix)
        }
    }

    /// A box inside the drawing, laid out at `origin`: what it draws is in its own coordinates, as a document's box is, so it is moved to where it lies inside the box around it.
    pub fn open_box(&mut self, origin: (f32, f32)) {
        let (x, y) = self.origins.last().copied().unwrap_or_default();
        self.body.push_str(&format!(
            "<g transform=\"translate({},{})\">",
            round(origin.0 - x),
            round(origin.1 - y)
        ));
        self.groups.push(Group::Placed);
        self.origins.push(origin);
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
        // A border sits inside the box, where a stroke straddles the edge it is given, so the shape it is drawn on is the box already pulled in by half the stroke.
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
            // SVG carries one radius, so four different corners is not a shape `<rect>` can be and the outline is drawn.
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
        // Laid out by the browser rather than placed as an SVG `<text>`: wrapping, alignment, line height and the clamp are all things a Telar text style can ask for and an SVG text run cannot do.
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

    /// `rect` already has `object-fit` applied; `size` is the source's own pixel dimensions, needed separately because a tile repeats at it and a nine-slice cuts by it.
    ///
    /// With no `href` the picture is drawn without its pixels: a page written ahead of time cannot carry the ones the app makes, and keeping the element lets the client that takes the page over give it just the address.
    pub fn image(
        &mut self,
        href: Option<&str>,
        size: (u32, u32),
        rect: Rect,
        raster: Raster,
        fill: ImageFill,
    ) {
        let (width, height) = (size.0 as f32, size.1 as f32);
        match fill {
            ImageFill::Stretch => {
                let tag = image_tag(href, rect, raster);
                self.body.push_str(&tag);
            }
            ImageFill::Tile { scale } => {
                if !(scale.is_finite() && scale > 0.0 && width > 0.0 && height > 0.0) {
                    return;
                }
                let id = self.def_id();
                let tile = Rect::new(0.0, 0.0, width * scale, height * scale);
                self.defs.push_str(&format!(
                    "<pattern id=\"{id}\" patternUnits=\"userSpaceOnUse\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\">{}</pattern>",
                    round(rect.x),
                    round(rect.y),
                    round(tile.width),
                    round(tile.height),
                    image_tag(href, tile, raster),
                ));
                self.body.push_str(&format!(
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"url(#{id})\"/>",
                    round(rect.x),
                    round(rect.y),
                    round(rect.width.max(0.0)),
                    round(rect.height.max(0.0)),
                ));
            }
            // Each piece is a viewport onto its cut of the picture, which an inner `<svg>` clips to without a clip path per piece.
            ImageFill::Slice(slice) => {
                let whole = Rect::new(0.0, 0.0, width, height);
                for piece in slice.pieces(size, rect) {
                    let (dest, source) = (piece.dest, piece.source);
                    self.body.push_str(&format!(
                        "<svg x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" viewBox=\"{} {} {} {}\" preserveAspectRatio=\"none\">{}</svg>",
                        round(dest.x),
                        round(dest.y),
                        round(dest.width),
                        round(dest.height),
                        round(source.x),
                        round(source.y),
                        round(source.width),
                        round(source.height),
                        image_tag(href, whole, raster),
                    ));
                }
            }
        }
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

    /// A layer in an SVG blends with what the same drawing put beneath it: the `<svg>` isolates its content from the page behind.
    ///
    /// A mask source is drawn into a `<mask>` definition rather than the picture, and the layer it applies to is a group shown through it: what the source covers, by its alpha, and nothing where it drew nothing.
    pub fn open_layer(&mut self, opacity: f32, blend: BlendMode, mask: LayerMask) {
        let blending = match blend {
            BlendMode::Normal => String::new(),
            other => format!(" style=\"mix-blend-mode:{}\"", other.css_name()),
        };
        match mask {
            LayerMask::Source => {
                let id = self.def_id();
                let drawn = std::mem::take(&mut self.body);
                self.body
                    .push_str(&format!("<g opacity=\"{}\">", round(opacity)));
                self.groups.push(Group::MaskSource { drawn, id });
            }
            LayerMask::Apply => {
                let masked = match self.pending_mask.take() {
                    Some(id) => format!(" mask=\"url(#{id})\""),
                    None => String::new(),
                };
                self.open(&format!(
                    "<g opacity=\"{}\"{masked}{blending}>",
                    round(opacity)
                ));
            }
            LayerMask::None => {
                self.open(&format!("<g opacity=\"{}\"{blending}>", round(opacity)));
            }
        }
    }

    /// Whether what is drawn now is a mask source, where a browser draws SVG shapes and not a document.
    pub fn in_mask(&self) -> bool {
        self.groups
            .iter()
            .any(|group| matches!(group, Group::MaskSource { .. }))
    }

    /// A one-line text as an SVG `<text>`, for a mask source: a browser does not draw a document inside a mask, so the `foreignObject` [`text`](Self::text) writes would cover nothing. `baseline` is how far below the top of its line box the text sits on its baseline.
    pub fn mask_text(&mut self, text: &str, rect: Rect, style: &TextStyle, baseline: f32) {
        if text.is_empty() {
            return;
        }
        let (x, anchor) = match style.text_align {
            renderer_core::TextAlign::Center => (rect.x + rect.width / 2.0, "middle"),
            renderer_core::TextAlign::End => (rect.x + rect.width, "end"),
            renderer_core::TextAlign::Start | renderer_core::TextAlign::Justify => {
                (rect.x, "start")
            }
        };
        let mut css = String::new();
        crate::paint::text_style(style, &mut css);
        let fill = match style.color {
            Paint::Solid(ink) => color(ink),
            Paint::Gradient(_) => "#fff".to_string(),
        };
        self.body.push_str(&format!(
            "<text x=\"{}\" y=\"{}\" text-anchor=\"{anchor}\" fill=\"{fill}\" style=\"{}\">{}</text>",
            round(x),
            round(rect.y + baseline),
            escape(&css),
            escape(text.lines().next().unwrap_or_default()),
        ));
    }

    pub fn close_group(&mut self) {
        let Some(group) = self.groups.pop() else {
            return;
        };
        self.body.push_str("</g>");
        if matches!(group, Group::Placed) {
            self.origins.pop();
        }
        if let Group::MaskSource { drawn, id } = group {
            let source = std::mem::replace(&mut self.body, drawn);
            // The region is the whole plane rather than the default box around the masked group, which would cut off whatever of the source lies outside it.
            self.defs.push_str(&format!(
                "<mask id=\"{id}\" maskUnits=\"userSpaceOnUse\" x=\"-100000\" y=\"-100000\" width=\"200000\" height=\"200000\" style=\"mask-type:alpha\">{source}</mask>"
            ));
            self.pending_mask = Some(id);
        }
    }

    /// The markup for this element's children, with everything still open closed.
    pub fn finish(mut self) -> String {
        while !self.groups.is_empty() {
            self.close_group();
        }
        if self.defs.is_empty() {
            return self.body;
        }
        format!("<defs>{}</defs>{}", self.defs, self.body)
    }

    fn open(&mut self, tag: &str) {
        self.body.push_str(tag);
        self.groups.push(Group::Plain);
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
        if let Some(dash) = &stroke.dash {
            let lengths: Vec<String> = dash.lengths().iter().map(|length| round(*length)).collect();
            attr(attrs, "stroke-dasharray", &lengths.join(" "));
            if dash.offset() != 0.0 {
                attr(attrs, "stroke-dashoffset", &round(dash.offset()));
            }
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
        // A blur radius is the width of the whole falloff, where a Gaussian is described by its deviation — the same halving CSS does for `box-shadow`.
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
/// Every other frame here is an inset `box-shadow`, which follows the radius and takes no room from the box — but a shadow carries a colour, and a gradient is not one. So the ring is drawn: the box's own outline with the inner edge punched out of it under the even-odd rule, which is the shape the rasterising backends fill for the same border.
///
/// Drawn in the box's own corner rather than where it stands on the surface, so the picture can be laid over it as a background of exactly its size — and the gradient moves with it, since its points were measured against the rect the widget drew.
pub fn frame_svg(
    rect: Rect,
    radius: BorderRadius,
    widths: [f32; 4],
    gradient: &Gradient,
) -> String {
    let outer = Rect::new(0.0, 0.0, rect.width, rect.height);
    let mut d = rounded_outline(outer, radius);
    // No interior means the frame swallowed the box, so the outer outline is the whole of it.
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
    // User space, not the default object bounding box: the points are the ones the widget drew with.
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

/// Markup-safe text. Every string that reaches this file is one the application chose — a label, a font family, a path a picture was loaded from — so none of it can be assumed to be markup already.
fn image_tag(href: Option<&str>, rect: Rect, raster: Raster) -> String {
    let rendering = match raster {
        Raster::Pixel => " style=\"image-rendering:pixelated\"",
        Raster::Smooth => "",
    };
    format!(
        "<image x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" preserveAspectRatio=\"none\"{rendering}{}/>",
        round(rect.x),
        round(rect.y),
        round(rect.width.max(0.0)),
        round(rect.height.max(0.0)),
        href.map(|href| format!(" href=\"{}\"", escape(href)))
            .unwrap_or_default(),
    )
}

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
#[path = "vector_test.rs"]
mod tests;
