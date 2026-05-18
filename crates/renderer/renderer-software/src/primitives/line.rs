use renderer_core::{LineCap, LineStyle, Point};

use crate::primitives::to_skia_color;

pub(crate) fn draw_line(
    pixmap: &mut tiny_skia::Pixmap,
    p1: Point,
    p2: Point,
    style: LineStyle,
    transform: tiny_skia::Transform,
    clip: Option<&tiny_skia::Mask>,
) {
    let mut pb = tiny_skia::PathBuilder::new();
    pb.move_to(p1.x, p1.y);
    pb.line_to(p2.x, p2.y);
    let Some(path) = pb.finish() else { return };

    let mut paint = tiny_skia::Paint::default();
    paint.set_color(to_skia_color(style.color));
    paint.anti_alias = true;

    let line_cap = match style.cap {
        LineCap::Butt => tiny_skia::LineCap::Butt,
        LineCap::Round => tiny_skia::LineCap::Round,
        LineCap::Square => tiny_skia::LineCap::Square,
    };
    let stroke = tiny_skia::Stroke {
        width: style.width,
        line_cap,
        ..Default::default()
    };

    pixmap.stroke_path(&path, &paint, &stroke, transform, clip);
}
