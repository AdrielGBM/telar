//! Draw commands as text: one line per command, stable across runs and machines, for a reviewer to read a change in.

use std::fmt::Write as _;

use geometry_core::{Point, Rect, Size};
use renderer_core::{
    Border, BorderRadius, Clamp, Color, DecorationLine, DecorationMetric, DrawCommand, FillRule,
    FontFamily, FontStyle, Gradient, GradientKind, ImageFill, LayerMask, LineCap, LineHeight,
    LineJoin, Paint, PathVerb, Raster, Shadow, Stroke, TextAlign, TextCase, TextShadow, TextStyle,
    TextWrap,
};

use crate::RecordedFrame;

/// `commands` as text, one line per command, each a keyword and then `key=value` fields.
///
/// Written for a line diff to read: geometry is rounded to hundredths of a pixel and colours resolved to `#rrggbbaa`, so float noise below what a screen shows never moves a line; a field at its default is left out, so a line names only what was set; and what a clip, matrix, layer or element encloses is indented one step under it, closed by an `end` line. An element names its role, its label where it has one, and its rect where the frame gives it one. Nothing that changes from run to run is written — no ids, no pointers, no image content hashes — so the same tree always gives the same text.
pub fn draw_text(commands: &[DrawCommand]) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    for command in commands {
        if closes(command) {
            depth = depth.saturating_sub(1);
        }
        for _ in 0..depth {
            out.push_str("  ");
        }
        write_command(&mut out, command);
        out.push('\n');
        if opens(command) {
            depth += 1;
        }
    }
    out
}

impl RecordedFrame {
    /// The frame as [`draw_text`] writes it, under a `frame` line with its size and the colour it was cleared to.
    pub fn to_text(&self) -> String {
        let mut out = format!("frame {}x{}", self.width, self.height);
        if let Some(clear) = self.clear {
            let _ = write!(out, " clear={}", color(clear));
        }
        out.push('\n');
        out.push_str(&draw_text(&self.commands));
        out
    }
}

fn opens(command: &DrawCommand) -> bool {
    matches!(
        command,
        DrawCommand::PushClip { .. }
            | DrawCommand::PushMatrix { .. }
            | DrawCommand::PushLayer { .. }
            | DrawCommand::PushElement { .. }
    )
}

fn closes(command: &DrawCommand) -> bool {
    matches!(
        command,
        DrawCommand::PopClip
            | DrawCommand::PopMatrix
            | DrawCommand::PopLayer
            | DrawCommand::PopElement
    )
}

fn write_command(out: &mut String, command: &DrawCommand) {
    match command {
        DrawCommand::Rect { rect: r, style } => {
            let _ = write!(out, "rect {}", rect(*r));
            if let Some(fill) = &style.fill {
                let _ = write!(out, " fill={}", paint(fill));
            }
            if let Some(border) = &style.border {
                write_border(out, border);
            }
            write_radius(out, "radius", style.radius);
            if let Some(cast) = &style.shadow {
                let _ = write!(out, " shadow={}", shadow(cast));
            }
        }
        DrawCommand::Text {
            text,
            spans,
            rect: r,
            style,
        } => {
            let _ = write!(out, "text {} {text:?}", rect(*r));
            for (key, value) in text_fields(style) {
                let _ = write!(out, " {key}={value}");
            }
            for span in spans.iter().flat_map(|spans| spans.iter()) {
                let over = span.over.over(style, Size::ZERO);
                let base = text_fields(style);
                let changed: Vec<String> = text_fields(&over)
                    .into_iter()
                    .filter(|field| !base.contains(field))
                    .map(|(key, value)| format!("{key}={value}"))
                    .collect();
                let _ = write!(
                    out,
                    " span={}..{}[{}]",
                    span.range.start,
                    span.range.end,
                    changed.join(" ")
                );
            }
        }
        DrawCommand::Image {
            data,
            rect: r,
            raster,
            fill,
        } => {
            let _ = write!(
                out,
                "image {} source={}x{}",
                rect(*r),
                data.width,
                data.height
            );
            match fill {
                ImageFill::Stretch => {}
                ImageFill::Tile { scale } => {
                    let _ = write!(out, " fill=tile scale={}", num(*scale));
                }
                ImageFill::Slice(slice) => {
                    let insets = slice.insets;
                    let _ = write!(
                        out,
                        " fill=slice insets={} scale={}",
                        list(&[insets.top, insets.right, insets.bottom, insets.left]),
                        num(slice.scale)
                    );
                }
            }
            if *raster == Raster::Pixel {
                out.push_str(" raster=pixel");
            }
        }
        DrawCommand::Line { p1, p2, style } => {
            let _ = write!(out, "line {} {}", point(*p1), point(*p2));
            write_stroke(out, style);
        }
        DrawCommand::Path { data, style } => {
            out.push_str("path");
            for verb in data.verbs() {
                let _ = match verb {
                    PathVerb::MoveTo(to) => write!(out, " M{}", point(*to)),
                    PathVerb::LineTo(to) => write!(out, " L{}", point(*to)),
                    PathVerb::QuadTo { ctrl, to } => {
                        write!(out, " Q{} {}", point(*ctrl), point(*to))
                    }
                    PathVerb::CubicTo { ctrl1, ctrl2, to } => {
                        write!(out, " C{} {} {}", point(*ctrl1), point(*ctrl2), point(*to))
                    }
                    PathVerb::Close => write!(out, " Z"),
                };
            }
            if let Some(fill) = &style.fill {
                let _ = write!(out, " fill={}", paint(fill));
            }
            if style.fill_rule == FillRule::EvenOdd {
                out.push_str(" rule=evenodd");
            }
            if let Some(stroke) = &style.stroke {
                write_stroke(out, stroke);
            }
            if let Some(cast) = &style.shadow {
                let _ = write!(out, " shadow={}", shadow(cast));
            }
        }
        DrawCommand::PushClip { rect: r, radius } => {
            let _ = write!(out, "clip {}", rect(*r));
            write_radius(out, "radius", *radius);
        }
        DrawCommand::PushMatrix { matrix } => {
            let _ = write!(out, "matrix {}", list(matrix));
        }
        DrawCommand::PushLayer {
            opacity,
            backdrop_blur,
            blend,
            mask,
        } => {
            out.push_str("layer");
            if *opacity != 1.0 {
                let _ = write!(out, " opacity={}", num(*opacity));
            }
            if *backdrop_blur != 0.0 {
                let _ = write!(out, " backdrop-blur={}", num(*backdrop_blur));
            }
            if *blend != renderer_core::BlendMode::Normal {
                let _ = write!(out, " blend={}", blend.css_name());
            }
            match mask {
                LayerMask::None => {}
                LayerMask::Source => out.push_str(" mask=source"),
                LayerMask::Apply => out.push_str(" mask=apply"),
            }
        }
        DrawCommand::PushElement { element } => {
            let _ = write!(out, "element {}", element.semantics.role.as_str());
            if element.rect.width != 0.0 || element.rect.height != 0.0 {
                let _ = write!(out, " {}", rect(element.rect));
            }
            if let Some(label) = &element.semantics.label {
                let _ = write!(out, " label={:?}", &**label);
            }
        }
        DrawCommand::PopClip
        | DrawCommand::PopMatrix
        | DrawCommand::PopLayer
        | DrawCommand::PopElement => out.push_str("end"),
    }
}

/// The fields a text line names, in a fixed order: size, family, weight and colour always, the rest only where they differ from [`TextStyle::new`]'s.
fn text_fields(style: &TextStyle) -> Vec<(&'static str, String)> {
    let mut fields = vec![
        ("size", num(style.font_size)),
        ("family", family(&style.font_family)),
        ("weight", style.font_weight.to_string()),
        ("color", paint(&style.color)),
    ];
    let mut add = |key: &'static str, value: Option<String>| {
        if let Some(value) = value {
            fields.push((key, value));
        }
    };
    add(
        "style",
        match style.font_style {
            FontStyle::Normal => None,
            FontStyle::Italic => Some("italic".into()),
            FontStyle::Oblique => Some("oblique".into()),
        },
    );
    add(
        "align",
        match style.text_align {
            TextAlign::Start => None,
            TextAlign::Center => Some("center".into()),
            TextAlign::End => Some("end".into()),
            TextAlign::Justify => Some("justify".into()),
        },
    );
    if let Clamp::Lines { max, ellipsis } = style.clamp {
        add("clamp", Some(max.to_string()));
        add("ellipsis", ellipsis.then(|| "true".into()));
    }
    add(
        "line-height",
        match style.line_height {
            LineHeight::Natural => None,
            LineHeight::Times(times) => Some(num(times)),
        },
    );
    add(
        "letter-spacing",
        (style.letter_spacing != 0.0).then(|| num(style.letter_spacing)),
    );
    add(
        "raster",
        (style.raster == Raster::Pixel).then(|| "pixel".into()),
    );
    add(
        "wrap",
        (style.text_wrap == TextWrap::NoWrap).then(|| "none".into()),
    );
    let variations: Vec<String> = style
        .font_variations
        .iter()
        .map(|(tag, value)| format!("{}:{}", tag.as_str(), num(value)))
        .collect();
    add(
        "variations",
        (!variations.is_empty()).then(|| variations.join(",")),
    );
    let features: Vec<String> = style
        .font_features
        .iter()
        .map(|(tag, value)| format!("{}:{value}", tag.as_str()))
        .collect();
    add(
        "features",
        (!features.is_empty()).then(|| features.join(",")),
    );
    add(
        "case",
        match style.text_case {
            TextCase::AsWritten => None,
            TextCase::Upper => Some("upper".into()),
            TextCase::Lower => Some("lower".into()),
            TextCase::Capitalize => Some("capitalize".into()),
        },
    );
    let decoration = &style.decoration;
    if decoration.line == DecorationLine::Underline {
        add("underline", Some(decoration_metric(decoration.thickness)));
        add(
            "underline-offset",
            Some(decoration_metric(decoration.offset)),
        );
        add("underline-color", decoration.color.as_ref().map(paint));
    }
    add("lang", style.lang.as_deref().map(str::to_string));
    add(
        "shadow",
        match &style.text_shadow {
            TextShadow::None => None,
            TextShadow::Cast(cast) => Some(shadow(cast)),
        },
    );
    fields
}

fn decoration_metric(metric: DecorationMetric) -> String {
    match metric {
        DecorationMetric::FromFont => "font".into(),
        DecorationMetric::Px(px) => num(px),
    }
}

fn family(family: &FontFamily) -> String {
    match family {
        FontFamily::SansSerif => "sans-serif".into(),
        FontFamily::Serif => "serif".into(),
        FontFamily::Monospace => "monospace".into(),
        FontFamily::SystemUi => "system-ui".into(),
        FontFamily::Cursive => "cursive".into(),
        FontFamily::Fantasy => "fantasy".into(),
        FontFamily::Named(name) => format!("{:?}", &**name),
        FontFamily::Stack(families) => families
            .iter()
            .map(self::family)
            .collect::<Vec<_>>()
            .join(","),
    }
}

fn write_border(out: &mut String, border: &Border) {
    let [top, right, bottom, left] = border.widths;
    let widths = if top == right && right == bottom && bottom == left {
        num(top)
    } else {
        list(&border.widths)
    };
    let _ = write!(
        out,
        " border={widths} border-color={}",
        paint(&border.paint)
    );
}

fn write_radius(out: &mut String, key: &str, radius: BorderRadius) {
    let corners = [
        radius.top_left,
        radius.top_right,
        radius.bottom_right,
        radius.bottom_left,
    ];
    if corners.iter().all(|corner| *corner == 0.0) {
        return;
    }
    let value = if corners.iter().all(|corner| *corner == corners[0]) {
        num(corners[0])
    } else {
        list(&corners)
    };
    let _ = write!(out, " {key}={value}");
}

fn write_stroke(out: &mut String, stroke: &Stroke) {
    let _ = write!(
        out,
        " stroke={} stroke-color={}",
        num(stroke.width),
        paint(&stroke.paint)
    );
    match stroke.cap {
        LineCap::Butt => {}
        LineCap::Round => out.push_str(" cap=round"),
        LineCap::Square => out.push_str(" cap=square"),
    }
    match stroke.join {
        LineJoin::Miter => {}
        LineJoin::Round => out.push_str(" join=round"),
        LineJoin::Bevel => out.push_str(" join=bevel"),
    }
    if let Some(dash) = &stroke.dash {
        let _ = write!(out, " dash={}@{}", list(dash.lengths()), num(dash.offset()));
    }
}

fn shadow(shadow: &Shadow) -> String {
    format!(
        "{},{},{},{},{}",
        num(shadow.offset_x),
        num(shadow.offset_y),
        num(shadow.blur_radius),
        num(shadow.spread),
        color(shadow.color)
    )
}

fn paint(paint: &Paint) -> String {
    match paint {
        Paint::Solid(solid) => color(*solid),
        Paint::Gradient(gradient) => self::gradient(gradient),
    }
}

fn gradient(gradient: &Gradient) -> String {
    let stops: Vec<String> = gradient
        .stops
        .active()
        .iter()
        .map(|stop| format!("{}@{}", color(stop.color), num(stop.position)))
        .collect();
    match gradient.kind {
        GradientKind::Linear { start, end } => {
            format!(
                "linear({}>{};{})",
                point(start),
                point(end),
                stops.join(";")
            )
        }
        GradientKind::Radial { center, radius } => format!(
            "radial({}r{};{})",
            point(center),
            num(radius),
            stops.join(";")
        ),
    }
}

/// `#rrggbbaa`, each channel rounded to the byte a rasteriser writes.
fn color(color: Color) -> String {
    let [r, g, b, a] = color.to_rgba8();
    format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
}

fn rect(rect: Rect) -> String {
    format!(
        "{},{} {}x{}",
        num(rect.x),
        num(rect.y),
        num(rect.width),
        num(rect.height)
    )
}

fn point(point: Point) -> String {
    format!("{},{}", num(point.x), num(point.y))
}

fn list(values: &[f32]) -> String {
    values.iter().map(|v| num(*v)).collect::<Vec<_>>().join(",")
}

/// `value` rounded to hundredths, with no trailing zeros and no negative zero.
pub(crate) fn num(value: f32) -> String {
    if !value.is_finite() {
        return value.to_string();
    }
    let rounded = (f64::from(value) * 100.0).round() / 100.0;
    if rounded == 0.0 {
        return "0".into();
    }
    rounded.to_string()
}

#[cfg(test)]
#[path = "text_test.rs"]
mod tests;
