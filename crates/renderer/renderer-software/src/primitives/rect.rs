use renderer_core::{BorderRadius, FillStyle, Rect, RectStyle};

use crate::renderer::to_skia_color;

pub(crate) fn build_rect_path(rect: Rect, radius: BorderRadius) -> Option<tiny_skia::Path> {
    let x = rect.x;
    let y = rect.y;
    let w = rect.width;
    let h = rect.height;

    if radius.is_zero() {
        let r = tiny_skia::Rect::from_xywh(x, y, w, h)?;
        let mut pb = tiny_skia::PathBuilder::new();
        pb.push_rect(r);
        return pb.finish();
    }

    let tl = radius.top_left.min(w / 2.0).min(h / 2.0);
    let tr = radius.top_right.min(w / 2.0).min(h / 2.0);
    let br = radius.bottom_right.min(w / 2.0).min(h / 2.0);
    let bl = radius.bottom_left.min(w / 2.0).min(h / 2.0);

    const K: f32 = 0.552_284_8;

    let mut pb = tiny_skia::PathBuilder::new();

    pb.move_to(x + tl, y);
    pb.line_to(x + w - tr, y);
    pb.cubic_to(
        x + w - tr + K * tr,
        y,
        x + w,
        y + tr - K * tr,
        x + w,
        y + tr,
    );
    pb.line_to(x + w, y + h - br);
    pb.cubic_to(
        x + w,
        y + h - br + K * br,
        x + w - br + K * br,
        y + h,
        x + w - br,
        y + h,
    );
    pb.line_to(x + bl, y + h);
    pb.cubic_to(
        x + bl - K * bl,
        y + h,
        x,
        y + h - bl + K * bl,
        x,
        y + h - bl,
    );
    pb.line_to(x, y + tl);
    pb.cubic_to(x, y + tl - K * tl, x + tl - K * tl, y, x + tl, y);
    pb.close();

    pb.finish()
}

pub(crate) fn draw_rect(
    pixmap: &mut tiny_skia::Pixmap,
    rect: Rect,
    style: &RectStyle,
    transform: tiny_skia::Transform,
    clip: Option<&tiny_skia::Mask>,
) {
    if let Some(fill_style) = style.fill {
        if let Some(path) = build_rect_path(rect, style.radius) {
            let color = match fill_style {
                FillStyle::Solid(c) => c,
            };
            let mut paint = tiny_skia::Paint::default();
            paint.set_color(to_skia_color(color));
            paint.anti_alias = true;
            pixmap.fill_path(&path, &paint, tiny_skia::FillRule::Winding, transform, clip);
        }
    }

    if let Some(s) = style.stroke {
        let half = s.width / 2.0;
        let inset = Rect::new(
            rect.x + half,
            rect.y + half,
            rect.width - s.width,
            rect.height - s.width,
        );
        let inset_radius = BorderRadius {
            top_left: (style.radius.top_left - half).max(0.0),
            top_right: (style.radius.top_right - half).max(0.0),
            bottom_right: (style.radius.bottom_right - half).max(0.0),
            bottom_left: (style.radius.bottom_left - half).max(0.0),
        };
        if let Some(path) = build_rect_path(inset, inset_radius) {
            let mut paint = tiny_skia::Paint::default();
            paint.set_color(to_skia_color(s.color));
            paint.anti_alias = true;
            let stroke = tiny_skia::Stroke {
                width: s.width,
                ..Default::default()
            };
            pixmap.stroke_path(&path, &paint, &stroke, transform, clip);
        }
    }
}
