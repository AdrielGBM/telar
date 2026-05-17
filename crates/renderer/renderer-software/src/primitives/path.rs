use renderer_core::{FillRule, LineCap, LineJoin, PathData, PathStyle, PathVerb};

use crate::renderer::to_skia_color;

fn build_skia_path(data: &PathData) -> Option<tiny_skia::Path> {
    let mut pb = tiny_skia::PathBuilder::new();
    for verb in &data.verbs {
        match verb {
            PathVerb::MoveTo(p) => pb.move_to(p.x, p.y),
            PathVerb::LineTo(p) => pb.line_to(p.x, p.y),
            PathVerb::QuadTo { ctrl, to } => pb.quad_to(ctrl.x, ctrl.y, to.x, to.y),
            PathVerb::CubicTo { ctrl1, ctrl2, to } => {
                pb.cubic_to(ctrl1.x, ctrl1.y, ctrl2.x, ctrl2.y, to.x, to.y)
            }
            PathVerb::Close => pb.close(),
        }
    }
    pb.finish()
}

fn to_skia_line_cap(cap: LineCap) -> tiny_skia::LineCap {
    match cap {
        LineCap::Butt => tiny_skia::LineCap::Butt,
        LineCap::Round => tiny_skia::LineCap::Round,
        LineCap::Square => tiny_skia::LineCap::Square,
    }
}

fn to_skia_line_join(join: LineJoin) -> tiny_skia::LineJoin {
    match join {
        LineJoin::Miter => tiny_skia::LineJoin::Miter,
        LineJoin::Round => tiny_skia::LineJoin::Round,
        LineJoin::Bevel => tiny_skia::LineJoin::Bevel,
    }
}

pub(crate) fn draw_path(
    pixmap: &mut tiny_skia::Pixmap,
    data: &PathData,
    style: &PathStyle,
    transform: tiny_skia::Transform,
    clip: Option<&tiny_skia::Mask>,
) {
    let Some(path) = build_skia_path(data) else {
        return;
    };

    if let Some(fill_style) = style.fill {
        let mut paint = tiny_skia::Paint::default();
        paint.set_color(to_skia_color(fill_style.color()));
        paint.anti_alias = true;
        let rule = match style.fill_rule {
            FillRule::Winding => tiny_skia::FillRule::Winding,
            FillRule::EvenOdd => tiny_skia::FillRule::EvenOdd,
        };
        pixmap.fill_path(&path, &paint, rule, transform, clip);
    }

    if let Some(s) = style.stroke {
        let mut paint = tiny_skia::Paint::default();
        paint.set_color(to_skia_color(s.color));
        paint.anti_alias = true;
        let line_cap = to_skia_line_cap(s.cap);
        let line_join = to_skia_line_join(s.join);
        let stroke = tiny_skia::Stroke {
            width: s.width,
            line_cap,
            line_join,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &paint, &stroke, transform, clip);
    }
}
