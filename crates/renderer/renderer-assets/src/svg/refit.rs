use std::sync::Arc;

use geometry_core::{Point, Rect};

use crate::image::apply_tint_premultiplied;
use renderer_core::{
    Color, DrawCommand, Gradient, GradientKind, ImageData, ImageFilter, Paint, PathStyle, Stroke,
};

/// Re-applies the runtime letterbox fit (`p' = p * s + offset`) to a baked vector display list.
///
/// The baked list lives in intrinsic viewBox space with original colors; here it is scaled/translated into widget space and, when `tint` is set, recolored exactly as the dynamic path would.
pub(super) fn refit_vector(
    baked: &[DrawCommand],
    s: f32,
    dx: f32,
    dy: f32,
    tint: Option<Color>,
) -> Vec<DrawCommand> {
    baked
        .iter()
        .map(|cmd| refit_command(cmd, s, dx, dy, tint))
        .collect()
}

fn refit_command(cmd: &DrawCommand, s: f32, dx: f32, dy: f32, tint: Option<Color>) -> DrawCommand {
    match cmd {
        DrawCommand::Path { data, style } => DrawCommand::Path {
            data: Arc::new(data.refit(s, dx, dy)),
            style: Arc::new(refit_style(style, s, dx, dy, tint)),
        },
        // PushLayer/PopLayer carry no geometry; a baked vector list contains nothing else.
        other => other.clone(),
    }
}

fn refit_style(style: &PathStyle, s: f32, dx: f32, dy: f32, tint: Option<Color>) -> PathStyle {
    PathStyle {
        fill: style.fill.map(|p| refit_paint(p, s, dx, dy, tint)),
        stroke: style.stroke.map(|st| refit_stroke(st, s, dx, dy, tint)),
        shadow: style.shadow,
        fill_rule: style.fill_rule,
    }
}

fn refit_stroke(stroke: Stroke, s: f32, dx: f32, dy: f32, tint: Option<Color>) -> Stroke {
    Stroke {
        paint: refit_paint(stroke.paint, s, dx, dy, tint),
        width: stroke.width * s,
        cap: stroke.cap,
        join: stroke.join,
    }
}

fn refit_paint(paint: Paint, s: f32, dx: f32, dy: f32, tint: Option<Color>) -> Paint {
    if let Some(tint) = tint {
        // srcIn tint mirrors `vector::convert_paint`: replace the paint with a solid whose alpha is the paint's effective opacity times the tint alpha. For a baked solid, that opacity is exactly its alpha; for a gradient (which the dynamic path also flattens under tint) we take the first stop's alpha, exact when stop opacities are 1 (the common case and all baked inputs here).
        let opacity = match paint {
            Paint::Solid(c) => c.a,
            Paint::Gradient(g) => g.stops.active().first().map_or(1.0, |st| st.color.a),
        };
        return Paint::Solid(tint.with_alpha(opacity * tint.a));
    }
    match paint {
        Paint::Solid(c) => Paint::Solid(c),
        Paint::Gradient(g) => Paint::Gradient(refit_gradient(g, s, dx, dy)),
    }
}

fn refit_gradient(g: Gradient, s: f32, dx: f32, dy: f32) -> Gradient {
    let map = |p: Point| Point::new(p.x * s + dx, p.y * s + dy);
    let kind = match g.kind {
        GradientKind::Linear { start, end } => GradientKind::Linear {
            start: map(start),
            end: map(end),
        },
        GradientKind::Radial { center, radius } => GradientKind::Radial {
            center: map(center),
            radius: radius * s,
        },
    };
    Gradient {
        kind,
        stops: g.stops,
    }
}

/// Draws a baked raster into the letterboxed content rect. The bitmap keeps its baked resolution; a tint multiplies a fresh copy of its premultiplied pixels, matching the dynamic raster fallback.
pub(super) fn refit_raster(
    image: &Arc<ImageData>,
    fitted_w: f32,
    fitted_h: f32,
    offset_x: f32,
    offset_y: f32,
    tint: Option<Color>,
) -> Vec<DrawCommand> {
    let data = match tint {
        None => Arc::clone(image),
        Some(tint) => {
            let mut pixels = image.pixels.clone();
            apply_tint_premultiplied(&mut pixels, tint);
            Arc::new(ImageData::from_premultiplied(
                pixels,
                image.width,
                image.height,
            ))
        }
    };
    vec![DrawCommand::Image {
        data,
        rect: Rect::new(offset_x, offset_y, fitted_w, fitted_h),
        filter: ImageFilter::Linear,
    }]
}
