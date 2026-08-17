use geometry_core::Point;

use usvg::tiny_skia_path::{Point as SkiaPoint, Transform as SkiaTransform};

use renderer_core::{
    Color, FillRule, Gradient, LineCap, LineJoin, Paint, PathData, PathStyle, Stroke,
};

use super::{Unsupported, VectorCommand};
use std::sync::Arc;

pub(super) fn convert_group(
    group: &usvg::Group,
    fit_ts: SkiaTransform,
    tint: Option<Color>,
    stroke_override: Option<f32>,
    out: &mut Vec<VectorCommand>,
) -> Result<(), Unsupported> {
    // Compositing we cannot express as a plain opacity layer forces the whole SVG to the raster fallback.
    if group.clip_path().is_some()
        || group.mask().is_some()
        || !group.filters().is_empty()
        || group.blend_mode() != usvg::BlendMode::Normal
    {
        return Err(Unsupported);
    }

    let opacity = group.opacity().get();
    let layered = opacity < 1.0;
    if layered {
        out.push(VectorCommand::PushLayer {
            opacity,
            backdrop_blur: 0.0,
        });
    }

    for node in group.children() {
        match node {
            usvg::Node::Group(g) => convert_group(g, fit_ts, tint, stroke_override, out)?,
            usvg::Node::Path(p) => convert_path(p, fit_ts, tint, stroke_override, out)?,
            // usvg flattens <text> to a group of paths (default `text` feature).
            usvg::Node::Text(t) => {
                convert_group(t.flattened(), fit_ts, tint, stroke_override, out)?
            }
            usvg::Node::Image(_) => return Err(Unsupported),
        }
    }

    if layered {
        out.push(VectorCommand::PopLayer);
    }
    Ok(())
}

fn convert_path(
    path: &usvg::Path,
    fit_ts: SkiaTransform,
    tint: Option<Color>,
    stroke_override: Option<f32>,
    out: &mut Vec<VectorCommand>,
) -> Result<(), Unsupported> {
    if !path.is_visible() {
        return Ok(());
    }

    let total = fit_ts.pre_concat(path.abs_transform());

    // Bake the full transform into the point coordinates instead of emitting a matrix: lyon (hardware backend) tessellates in path space with a fixed tolerance, so a large scale matrix would facet curves.
    let mut data = PathData::new();
    for seg in path.data().segments() {
        use usvg::tiny_skia_path::PathSegment;
        match seg {
            PathSegment::MoveTo(p) => data = data.move_to(map_pt(&total, p)),
            PathSegment::LineTo(p) => data = data.line_to(map_pt(&total, p)),
            PathSegment::QuadTo(c, p) => data = data.quad_to(map_pt(&total, c), map_pt(&total, p)),
            PathSegment::CubicTo(c1, c2, p) => {
                data = data.cubic_to(map_pt(&total, c1), map_pt(&total, c2), map_pt(&total, p))
            }
            PathSegment::Close => data = data.close(),
        }
    }

    let scale = uniform_scale(&total);

    let mut style = PathStyle::default();
    if let Some(fill) = path.fill() {
        style.fill = Some(convert_paint(
            fill.paint(),
            fill.opacity().get(),
            &total,
            tint,
            true,
        )?);
        style.fill_rule = match fill.rule() {
            usvg::FillRule::NonZero => FillRule::Winding,
            usvg::FillRule::EvenOdd => FillRule::EvenOdd,
        };
    }
    if let Some(stroke) = path.stroke() {
        // No dashed-stroke primitive.
        if stroke.dasharray().is_some() {
            return Err(Unsupported);
        }
        let paint = convert_paint(stroke.paint(), stroke.opacity().get(), &total, tint, false)?;
        // A theme icon-stroke token overrides the glyph's own stroke width in userspace units (e.g. Lucide's 2), so it still scales into widget space by the same fit `scale`.
        let width = stroke_override.unwrap_or_else(|| stroke.width().get());
        style.stroke = Some(Stroke {
            paint,
            width: width * scale,
            cap: map_cap(stroke.linecap()),
            join: map_join(stroke.linejoin()),
        });
    }

    out.push(VectorCommand::Path {
        data: Arc::new(data),
        style: Arc::new(style),
    });
    Ok(())
}

fn convert_paint(
    paint: &usvg::Paint,
    opacity: f32,
    total: &SkiaTransform,
    tint: Option<Color>,
    allow_gradient: bool,
) -> Result<Paint, Unsupported> {
    // A pattern has its own coverage we cannot reproduce vectorially, even under a tint.
    if let usvg::Paint::Pattern(_) = paint {
        return Err(Unsupported);
    }
    if let Some(tint) = tint {
        // srcIn tint: replace the color, keep the paint's effective alpha (gradient alpha variation is approximated as flat).
        return Ok(Paint::Solid(tint.with_alpha(opacity * tint.a)));
    }
    match paint {
        usvg::Paint::Color(c) => Ok(Paint::Solid(color_from(*c, opacity))),
        usvg::Paint::LinearGradient(lg) if allow_gradient => convert_linear(lg, opacity, total),
        usvg::Paint::RadialGradient(rg) if allow_gradient => convert_radial(rg, opacity, total),
        // Gradient on a stroke: the hardware backend paints strokes with a solid color only, so rasterize rather than silently flatten.
        _ => Err(Unsupported),
    }
}

fn convert_linear(
    lg: &usvg::LinearGradient,
    opacity: f32,
    total: &SkiaTransform,
) -> Result<Paint, Unsupported> {
    let stops = convert_stops(lg.stops(), opacity)?;
    let grad_ts = total.pre_concat(lg.transform());
    let start = map_pt(&grad_ts, SkiaPoint::from_xy(lg.x1(), lg.y1()));
    let end = map_pt(&grad_ts, SkiaPoint::from_xy(lg.x2(), lg.y2()));
    Ok(Paint::Gradient(Gradient::linear(start, end, &stops)))
}

fn convert_radial(
    rg: &usvg::RadialGradient,
    opacity: f32,
    total: &SkiaTransform,
) -> Result<Paint, Unsupported> {
    // Focal radial gradients (focus differs from the center) have no primitive.
    if rg.fx() != rg.cx() || rg.fy() != rg.cy() || rg.fr().get() != 0.0 {
        return Err(Unsupported);
    }
    let grad_ts = total.pre_concat(rg.transform());
    // Only a similarity (uniform scale + rotation + translation) keeps the gradient circular; skew/non-uniform scale would need an ellipse.
    if !is_conformal(&grad_ts) {
        return Err(Unsupported);
    }
    let stops = convert_stops(rg.stops(), opacity)?;
    let center = map_pt(&grad_ts, SkiaPoint::from_xy(rg.cx(), rg.cy()));
    let radius = rg.r().get() * uniform_scale(&grad_ts);
    Ok(Paint::Gradient(Gradient::radial(center, radius, &stops)))
}

fn convert_stops(stops: &[usvg::Stop], opacity: f32) -> Result<Vec<(f32, Color)>, Unsupported> {
    // `GradientStops` caps at 8. The hardware backend shader only reads 4, but that is a backend limitation left as-is.
    if stops.len() > 8 {
        return Err(Unsupported);
    }
    Ok(stops
        .iter()
        .map(|s| {
            let color = color_from(s.color(), s.opacity().get() * opacity);
            (s.offset().get(), color)
        })
        .collect())
}

fn color_from(c: usvg::Color, alpha: f32) -> Color {
    Color::from_rgb_u8(c.red, c.green, c.blue).with_alpha(alpha)
}

fn map_pt(ts: &SkiaTransform, p: SkiaPoint) -> Point {
    let mut p = p;
    ts.map_point(&mut p);
    Point::new(p.x, p.y)
}

fn uniform_scale(ts: &SkiaTransform) -> f32 {
    // sqrt of |determinant|: the geometric-mean scale, exact for a similarity and reducing to the fit scale when the node transform is identity.
    (ts.sx * ts.sy - ts.kx * ts.ky).abs().sqrt()
}

fn is_conformal(ts: &SkiaTransform) -> bool {
    // Linear part is [[sx, kx], [ky, sy]]; a similarity has equal-length, orthogonal columns.
    let (col0, col1, dot) = (
        ts.sx * ts.sx + ts.ky * ts.ky,
        ts.kx * ts.kx + ts.sy * ts.sy,
        ts.sx * ts.kx + ts.ky * ts.sy,
    );
    let scale = col0.max(col1).max(1.0);
    let tol = 1e-3 * scale;
    (col0 - col1).abs() <= tol && dot.abs() <= tol
}

fn map_cap(cap: usvg::LineCap) -> LineCap {
    match cap {
        usvg::LineCap::Butt => LineCap::Butt,
        usvg::LineCap::Round => LineCap::Round,
        usvg::LineCap::Square => LineCap::Square,
    }
}

fn map_join(join: usvg::LineJoin) -> LineJoin {
    match join {
        usvg::LineJoin::Miter => LineJoin::Miter,
        // No miter-clip primitive; plain miter is the closest match.
        usvg::LineJoin::MiterClip => LineJoin::Miter,
        usvg::LineJoin::Round => LineJoin::Round,
        usvg::LineJoin::Bevel => LineJoin::Bevel,
    }
}
