use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use geometry_core::{Point, Rect};
use rustc_hash::{FxHashMap, FxHasher};

use usvg::tiny_skia_path::{Point as SkiaPoint, Transform as SkiaTransform};

use crate::{
    Color, DrawCommand, FillRule, Gradient, ImageData, ImageFilter, LineCap, LineJoin, Paint,
    PathData, PathStyle, Stroke,
};

#[derive(Debug, Clone, thiserror::Error)]
#[error("failed to parse SVG: {0}")]
pub struct SvgError(String);

// Memo key: (width bits, height bits, tint as packed rgba bits or None). f32 goes through `to_bits` so it is Eq/Hash.
type MemoKey = (u32, u32, Option<[u32; 4]>);

/// A parsed SVG document that converts to the renderer's `DrawCommand`s.
///
/// The vector path is the common case; SVGs using features without a drawing primitive fall back to a single rasterized `DrawCommand::Image`.
pub struct SvgData {
    // Content-derived (not a counter) so rebuilding the same SVG keeps a stable id for downstream caches.
    id: u64,
    // Retained for the raster fallback path.
    tree: usvg::Tree,
    // Intrinsic size in px (`tree.size()`).
    size: (f32, f32),
    memo: Mutex<FxHashMap<MemoKey, Arc<Vec<DrawCommand>>>>,
}

// Marks conversion of a feature we have no primitive for; the caller then rasterizes the whole tree.
struct Unsupported;

impl SvgData {
    // Named `from_str` per the public API; it is fallible on content, not a `FromStr` impl.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(svg: &str) -> Result<Self, SvgError> {
        let opt = usvg::Options::default();
        let tree = usvg::Tree::from_str(svg, &opt).map_err(|e| SvgError(e.to_string()))?;
        let size = tree.size();
        let mut hasher = FxHasher::default();
        svg.as_bytes().hash(&mut hasher);
        Ok(Self {
            id: hasher.finish(),
            tree,
            size: (size.width(), size.height()),
            memo: Mutex::new(FxHashMap::default()),
        })
    }

    /// Stable, content-derived id. Same content parses to the same id across instances.
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn intrinsic_size(&self) -> (f32, f32) {
        self.size
    }

    /// Display list in the widget's LOCAL space (`0,0..width,height`), memoized per `(width, height, tint)`.
    pub fn commands_for(
        &self,
        width: f32,
        height: f32,
        tint: Option<Color>,
    ) -> Arc<Vec<DrawCommand>> {
        let key: MemoKey = (
            width.to_bits(),
            height.to_bits(),
            tint.map(|c| [c.r.to_bits(), c.g.to_bits(), c.b.to_bits(), c.a.to_bits()]),
        );
        let mut memo = self.memo.lock().unwrap();
        if let Some(cached) = memo.get(&key) {
            return Arc::clone(cached);
        }
        let commands = Arc::new(self.build_commands(width, height, tint));
        // Simple bound: drop the whole cache rather than track an LRU; these display lists are cheap to rebuild.
        if memo.len() >= 16 {
            memo.clear();
        }
        memo.insert(key, Arc::clone(&commands));
        commands
    }

    fn build_commands(&self, width: f32, height: f32, tint: Option<Color>) -> Vec<DrawCommand> {
        let (vb_w, vb_h) = self.size;
        if vb_w <= 0.0 || vb_h <= 0.0 || width <= 0.0 || height <= 0.0 {
            return Vec::new();
        }
        // Uniform scale, centered (xMidYMid meet letterbox).
        let s = (width / vb_w).min(height / vb_h);
        let fitted_w = vb_w * s;
        let fitted_h = vb_h * s;
        let offset_x = (width - fitted_w) * 0.5;
        let offset_y = (height - fitted_h) * 0.5;
        let fit_ts = SkiaTransform::from_row(s, 0.0, 0.0, s, offset_x, offset_y);

        let mut out = Vec::new();
        match convert_group(self.tree.root(), fit_ts, tint, &mut out) {
            Ok(()) => out,
            Err(Unsupported) => self.raster_fallback(fitted_w, fitted_h, offset_x, offset_y, tint),
        }
    }

    fn raster_fallback(
        &self,
        fitted_w: f32,
        fitted_h: f32,
        offset_x: f32,
        offset_y: f32,
        tint: Option<Color>,
    ) -> Vec<DrawCommand> {
        let (vb_w, vb_h) = self.size;
        // Render at 2x: this layer does not know the display scale factor, so 2x keeps icons crisp on HiDPI.
        const DENSITY: f32 = 2.0;
        const MAX_SIDE: f32 = 4096.0;
        let mut px_w = (fitted_w * DENSITY).ceil();
        let mut px_h = (fitted_h * DENSITY).ceil();
        let max_side = px_w.max(px_h);
        if max_side > MAX_SIDE {
            let k = MAX_SIDE / max_side;
            px_w = (px_w * k).floor();
            px_h = (px_h * k).floor();
        }
        let pw = (px_w as u32).max(1);
        let ph = (px_h as u32).max(1);

        let Some(mut pixmap) = resvg::tiny_skia::Pixmap::new(pw, ph) else {
            return Vec::new();
        };
        // The pixmap covers exactly the letterboxed content rect, so map the whole intrinsic viewBox onto it.
        let render_ts = resvg::tiny_skia::Transform::from_scale(pw as f32 / vb_w, ph as f32 / vb_h);
        resvg::render(&self.tree, render_ts, &mut pixmap.as_mut());

        let mut pixels = pixmap.take();
        if let Some(tint) = tint {
            apply_tint_premultiplied(&mut pixels, tint);
        }
        // resvg's Pixmap is already premultiplied RGBA8; use the constructor that skips premultiplication.
        let data = ImageData::from_premultiplied(pixels, pw, ph);
        vec![DrawCommand::Image {
            data: Arc::new(data),
            rect: Rect::new(offset_x, offset_y, fitted_w, fitted_h),
            filter: ImageFilter::Linear,
        }]
    }
}

fn convert_group(
    group: &usvg::Group,
    fit_ts: SkiaTransform,
    tint: Option<Color>,
    out: &mut Vec<DrawCommand>,
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
        out.push(DrawCommand::PushLayer {
            opacity,
            backdrop_blur: 0.0,
        });
    }

    for node in group.children() {
        match node {
            usvg::Node::Group(g) => convert_group(g, fit_ts, tint, out)?,
            usvg::Node::Path(p) => convert_path(p, fit_ts, tint, out)?,
            // usvg flattens <text> to a group of paths (default `text` feature).
            usvg::Node::Text(t) => convert_group(t.flattened(), fit_ts, tint, out)?,
            usvg::Node::Image(_) => return Err(Unsupported),
        }
    }

    if layered {
        out.push(DrawCommand::PopLayer);
    }
    Ok(())
}

fn convert_path(
    path: &usvg::Path,
    fit_ts: SkiaTransform,
    tint: Option<Color>,
    out: &mut Vec<DrawCommand>,
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
        style.stroke = Some(Stroke {
            paint,
            width: stroke.width().get() * scale,
            cap: map_cap(stroke.linecap()),
            join: map_join(stroke.linejoin()),
        });
    }

    out.push(DrawCommand::Path {
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

fn apply_tint_premultiplied(pixels: &mut [u8], tint: Color) {
    for px in pixels.chunks_exact_mut(4) {
        // Buffer is premultiplied, so its alpha byte already equals the source coverage.
        let coverage = px[3] as f32 / 255.0;
        let out_a = coverage * tint.a;
        px[0] = (tint.r * out_a * 255.0).round().clamp(0.0, 255.0) as u8;
        px[1] = (tint.g * out_a * 255.0).round().clamp(0.0, 255.0) as u8;
        px[2] = (tint.b * out_a * 255.0).round().clamp(0.0, 255.0) as u8;
        px[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GradientKind, PathVerb};

    // SvgData is shared across threads via Arc.
    const _: fn() = || {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SvgData>();
    };

    fn only_path(cmds: &[DrawCommand]) -> (&PathData, &PathStyle) {
        match cmds.iter().find(|c| matches!(c, DrawCommand::Path { .. })) {
            Some(DrawCommand::Path { data, style }) => (data, style),
            _ => panic!("expected a Path command, got {cmds:?}"),
        }
    }

    #[test]
    fn from_str_parses_and_reports_intrinsic_size() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="16"><rect width="24" height="16"/></svg>"##;
        let data = SvgData::from_str(svg).unwrap();
        let (w, h) = data.intrinsic_size();
        assert!((w - 24.0).abs() < 1e-3, "width {w}");
        assert!((h - 16.0).abs() < 1e-3, "height {h}");
    }

    #[test]
    fn invalid_svg_returns_err() {
        assert!(SvgData::from_str("not an svg at all").is_err());
    }

    #[test]
    fn solid_path_scales_and_centers() {
        // 10x10 viewBox, a filled rect covering the whole box, rendered into a 20x40 widget.
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" width="10" height="10"><rect x="0" y="0" width="10" height="10" fill="#ff0000"/></svg>"##;
        let data = SvgData::from_str(svg).unwrap();
        let cmds = data.commands_for(20.0, 40.0, None);
        let (path, style) = only_path(&cmds);

        // Fit scale is min(20/10, 40/10) = 2, centered vertically: offset_y = (40 - 20)/2 = 10.
        let xs: Vec<Point> = path
            .verbs()
            .iter()
            .filter_map(|v| match v {
                PathVerb::MoveTo(p) | PathVerb::LineTo(p) => Some(*p),
                _ => None,
            })
            .collect();
        let min_x = xs.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
        let max_x = xs.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max);
        let min_y = xs.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
        let max_y = xs.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max);
        assert!((min_x - 0.0).abs() < 1e-2, "min_x {min_x}");
        assert!((max_x - 20.0).abs() < 1e-2, "max_x {max_x}");
        assert!((min_y - 10.0).abs() < 1e-2, "min_y {min_y}");
        assert!((max_y - 30.0).abs() < 1e-2, "max_y {max_y}");

        match style.fill {
            Some(Paint::Solid(c)) => {
                assert!((c.r - 1.0).abs() < 1e-3 && c.g.abs() < 1e-3 && c.b.abs() < 1e-3);
                assert!((c.a - 1.0).abs() < 1e-3);
            }
            other => panic!("expected solid fill, got {other:?}"),
        }
    }

    #[test]
    fn group_opacity_emits_balanced_layer() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" width="10" height="10"><g opacity="0.5"><rect width="10" height="10" fill="#00ff00"/></g></svg>"##;
        let data = SvgData::from_str(svg).unwrap();
        let cmds = data.commands_for(10.0, 10.0, None);
        let pushes = cmds
            .iter()
            .filter(|c| matches!(c, DrawCommand::PushLayer { .. }))
            .count();
        let pops = cmds
            .iter()
            .filter(|c| matches!(c, DrawCommand::PopLayer))
            .count();
        assert_eq!(pushes, 1, "one PushLayer expected: {cmds:?}");
        assert_eq!(pops, 1, "one PopLayer expected: {cmds:?}");
        // PushLayer must precede PopLayer.
        let push_idx = cmds
            .iter()
            .position(|c| matches!(c, DrawCommand::PushLayer { .. }))
            .unwrap();
        let pop_idx = cmds
            .iter()
            .position(|c| matches!(c, DrawCommand::PopLayer))
            .unwrap();
        assert!(push_idx < pop_idx);
    }

    #[test]
    fn linear_gradient_becomes_gradient_paint() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" width="10" height="10">
            <defs><linearGradient id="g" gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="10" y2="0">
              <stop offset="0" stop-color="#000000"/><stop offset="1" stop-color="#ffffff"/>
            </linearGradient></defs>
            <rect width="10" height="10" fill="url(#g)"/></svg>"##;
        let data = SvgData::from_str(svg).unwrap();
        let cmds = data.commands_for(10.0, 10.0, None);
        let (_, style) = only_path(&cmds);
        match style.fill {
            Some(Paint::Gradient(g)) => match g.kind {
                GradientKind::Linear { start, end } => {
                    assert!((start.x - 0.0).abs() < 1e-2, "start.x {}", start.x);
                    assert!((end.x - 10.0).abs() < 1e-2, "end.x {}", end.x);
                }
                other => panic!("expected linear gradient, got {other:?}"),
            },
            other => panic!("expected gradient fill, got {other:?}"),
        }
    }

    #[test]
    fn filter_falls_back_to_raster_image() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" width="10" height="10">
            <defs><filter id="b"><feGaussianBlur stdDeviation="1"/></filter></defs>
            <g filter="url(#b)"><rect width="10" height="10" fill="#ff0000"/></g></svg>"##;
        let data = SvgData::from_str(svg).unwrap();
        let cmds = data.commands_for(10.0, 10.0, None);
        assert_eq!(
            cmds.len(),
            1,
            "fallback should be a single command: {cmds:?}"
        );
        match &cmds[0] {
            DrawCommand::Image { data, .. } => {
                // 10x10 fitted content at 2x density.
                assert_eq!(data.width, 20);
                assert_eq!(data.height, 20);
            }
            other => panic!("expected Image fallback, got {other:?}"),
        }
    }

    #[test]
    fn tint_replaces_vector_paint() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" width="10" height="10"><rect width="10" height="10" fill="#ff0000"/></svg>"##;
        let data = SvgData::from_str(svg).unwrap();
        let tint = Color::rgba(0.0, 0.0, 1.0, 1.0);
        let cmds = data.commands_for(10.0, 10.0, Some(tint));
        let (_, style) = only_path(&cmds);
        match style.fill {
            Some(Paint::Solid(c)) => {
                assert!(c.b > 0.9 && c.r < 0.1 && c.g < 0.1, "tinted color {c:?}");
            }
            other => panic!("expected tinted solid fill, got {other:?}"),
        }
    }

    #[test]
    fn commands_for_is_memoized() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" width="10" height="10"><rect width="10" height="10" fill="#ff0000"/></svg>"##;
        let data = SvgData::from_str(svg).unwrap();
        let a = data.commands_for(10.0, 10.0, None);
        let b = data.commands_for(10.0, 10.0, None);
        assert!(Arc::ptr_eq(&a, &b), "same args must return the same Arc");
        let c = data.commands_for(20.0, 20.0, None);
        assert!(
            !Arc::ptr_eq(&a, &c),
            "different args must return a different Arc"
        );
    }

    #[test]
    fn id_is_stable_across_instances() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" width="10" height="10"><rect width="10" height="10"/></svg>"##;
        assert_eq!(
            SvgData::from_str(svg).unwrap().id(),
            SvgData::from_str(svg).unwrap().id()
        );
    }
}
