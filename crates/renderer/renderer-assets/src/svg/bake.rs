use std::sync::Arc;

use usvg::tiny_skia_path::Transform as SkiaTransform;

use crate::image::byte_string_literal;
use renderer_core::{
    Color, DrawCommand, FillRule, Gradient, GradientKind, ImageData, LineCap, LineJoin, Paint,
    PathData, PathStyle, PathVerb, Stroke,
};

use super::raster::raster_px;
use super::vector::convert_group;
use super::{BakedSvg, SvgError, Unsupported};

/// Parses `content` and converts it to a `BakedSvg`: a vector display list when every feature has a primitive, otherwise the whole document rasterized. Shared by `bake_to_source` and the equivalence tests.
pub(crate) fn bake(content: &str) -> Result<((f32, f32), BakedSvg), SvgError> {
    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_str(content, &opt).map_err(|e| SvgError(e.to_string()))?;
    let size = tree.size();
    let intrinsic = (size.width(), size.height());

    let mut out = Vec::new();
    // Identity fit: baking stays in intrinsic viewBox space; the runtime re-fit applies the letterbox.
    match convert_group(tree.root(), SkiaTransform::identity(), None, None, &mut out) {
        Ok(()) => Ok((intrinsic, BakedSvg::Vector(out))),
        Err(Unsupported) => {
            let (image, raster_size) = rasterize(&tree, intrinsic)?;
            Ok((
                intrinsic,
                BakedSvg::Raster {
                    image: Arc::new(image),
                    raster_size,
                },
            ))
        }
    }
}

fn rasterize(
    tree: &usvg::Tree,
    intrinsic: (f32, f32),
) -> Result<(ImageData, (f32, f32)), SvgError> {
    let (vb_w, vb_h) = intrinsic;
    let (pw, ph) = raster_px(vb_w, vb_h);

    let mut pixmap = resvg::tiny_skia::Pixmap::new(pw, ph)
        .ok_or_else(|| SvgError("failed to allocate raster pixmap".into()))?;
    let render_ts = resvg::tiny_skia::Transform::from_scale(pw as f32 / vb_w, ph as f32 / vb_h);
    resvg::render(tree, render_ts, &mut pixmap.as_mut());
    let pixels = pixmap.take();
    let image = ImageData::from_premultiplied(pixels, pw, ph);
    Ok((image, (pw as f32, ph as f32)))
}

/// Build-time entry point: bake `content` and emit a Rust expression that reconstructs the equivalent `SvgData` with no runtime SVG dependency.
pub fn bake_to_source(content: &str) -> Result<String, SvgError> {
    let (size, baked) = bake(content)?;
    Ok(serialize(size, &baked))
}

// Emits bare type names (`SvgData`, `DrawCommand`, `Point`, …) because the transpiler drops this expression into generated code that does `use telar::*`, whose facade re-exports every renderer-core / geometry-core type unqualified.
fn serialize(size: (f32, f32), baked: &BakedSvg) -> String {
    match baked {
        BakedSvg::Vector(cmds) => {
            let mut s = String::from("SvgData::from_baked_vector(");
            s.push_str(&ser_size(size));
            s.push_str(", vec![");
            for cmd in cmds {
                s.push_str(&ser_command(cmd));
                s.push(',');
            }
            s.push_str("])");
            s
        }
        BakedSvg::Raster { image, raster_size } => format!(
            "SvgData::from_baked_raster({}, ImageData::from_premultiplied({}.to_vec(), {}, {}), {})",
            ser_size(size),
            byte_string_literal(image.pixels()),
            image.width,
            image.height,
            ser_size(*raster_size),
        ),
    }
}

fn ser_command(cmd: &DrawCommand) -> String {
    match cmd {
        DrawCommand::Path { data, style } => format!(
            "DrawCommand::Path {{ data: std::sync::Arc::new({}), style: std::sync::Arc::new({}) }}",
            ser_path_data(data),
            ser_path_style(style),
        ),
        DrawCommand::PushLayer {
            opacity,
            backdrop_blur,
        } => format!(
            "DrawCommand::PushLayer {{ opacity: {}, backdrop_blur: {} }}",
            fmt_f32(*opacity),
            fmt_f32(*backdrop_blur),
        ),
        DrawCommand::PopLayer => "DrawCommand::PopLayer".to_string(),
        // `convert_group` only ever emits Path / PushLayer / PopLayer into a baked vector list.
        _ => unreachable!("baked vector list contains only Path/PushLayer/PopLayer"),
    }
}

fn ser_path_data(data: &PathData) -> String {
    let mut s = String::from("PathData::new()");
    for verb in data.verbs() {
        match verb {
            PathVerb::MoveTo(p) => s.push_str(&format!(".move_to({})", ser_point(p))),
            PathVerb::LineTo(p) => s.push_str(&format!(".line_to({})", ser_point(p))),
            PathVerb::QuadTo { ctrl, to } => {
                s.push_str(&format!(".quad_to({}, {})", ser_point(ctrl), ser_point(to)))
            }
            PathVerb::CubicTo { ctrl1, ctrl2, to } => s.push_str(&format!(
                ".cubic_to({}, {}, {})",
                ser_point(ctrl1),
                ser_point(ctrl2),
                ser_point(to),
            )),
            PathVerb::Close => s.push_str(".close()"),
        }
    }
    s
}

fn ser_path_style(style: &PathStyle) -> String {
    format!(
        "PathStyle {{ fill: {}, stroke: {}, shadow: None, fill_rule: {} }}",
        ser_opt_paint(&style.fill),
        ser_opt_stroke(&style.stroke),
        ser_fill_rule(style.fill_rule),
    )
}

fn ser_opt_paint(paint: &Option<Paint>) -> String {
    match paint {
        None => "None".to_string(),
        Some(p) => format!("Some({})", ser_paint(p)),
    }
}

fn ser_opt_stroke(stroke: &Option<Stroke>) -> String {
    match stroke {
        None => "None".to_string(),
        Some(st) => format!(
            "Some(Stroke {{ paint: {}, width: {}, cap: {}, join: {} }})",
            ser_paint(&st.paint),
            fmt_f32(st.width),
            ser_cap(st.cap),
            ser_join(st.join),
        ),
    }
}

fn ser_paint(paint: &Paint) -> String {
    match paint {
        Paint::Solid(c) => format!("Paint::Solid({})", ser_color(*c)),
        Paint::Gradient(g) => format!("Paint::Gradient({})", ser_gradient(g)),
    }
}

fn ser_gradient(g: &Gradient) -> String {
    let stops = ser_stops(g);
    match g.kind {
        GradientKind::Linear { start, end } => format!(
            "Gradient::linear({}, {}, {})",
            ser_point(&start),
            ser_point(&end),
            stops,
        ),
        GradientKind::Radial { center, radius } => format!(
            "Gradient::radial({}, {}, {})",
            ser_point(&center),
            fmt_f32(radius),
            stops,
        ),
    }
}

fn ser_stops(g: &Gradient) -> String {
    let mut s = String::from("&[");
    for stop in g.stops.active() {
        s.push_str(&format!(
            "({}, {}),",
            fmt_f32(stop.position),
            ser_color(stop.color)
        ));
    }
    s.push(']');
    s
}

fn ser_color(c: Color) -> String {
    format!(
        "Color::rgba({}, {}, {}, {})",
        fmt_f32(c.r),
        fmt_f32(c.g),
        fmt_f32(c.b),
        fmt_f32(c.a),
    )
}

fn ser_cap(cap: LineCap) -> String {
    match cap {
        LineCap::Butt => "LineCap::Butt",
        LineCap::Round => "LineCap::Round",
        LineCap::Square => "LineCap::Square",
    }
    .to_string()
}

fn ser_join(join: LineJoin) -> String {
    match join {
        LineJoin::Miter => "LineJoin::Miter",
        LineJoin::Round => "LineJoin::Round",
        LineJoin::Bevel => "LineJoin::Bevel",
    }
    .to_string()
}

fn ser_fill_rule(rule: FillRule) -> String {
    match rule {
        FillRule::Winding => "FillRule::Winding",
        FillRule::EvenOdd => "FillRule::EvenOdd",
    }
    .to_string()
}

fn ser_point(p: &geometry_core::Point) -> String {
    format!("Point::new({}, {})", fmt_f32(p.x), fmt_f32(p.y))
}

fn ser_size(size: (f32, f32)) -> String {
    format!("({}, {})", fmt_f32(size.0), fmt_f32(size.1))
}

// `{:?}` yields the shortest string that round-trips to the same f32; the `f32` suffix pins the literal's type so no `f64 -> f32` rounding can creep in.
fn fmt_f32(v: f32) -> String {
    format!("{v:?}f32")
}
