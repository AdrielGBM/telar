use geometry_core::Point;
use renderer_core::LineStyle;

use crate::primitives::{to_skia_color, to_skia_line_cap};

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

    let stroke = tiny_skia::Stroke {
        width: style.width,
        line_cap: to_skia_line_cap(style.cap),
        ..Default::default()
    };

    pixmap.stroke_path(&path, &paint, &stroke, transform, clip);
}
