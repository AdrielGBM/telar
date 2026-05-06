use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use renderer_core::{BorderRadius, Color, Rect, Stroke};

use crate::renderer::{SoftwareRenderer, to_skia_color};

pub(crate) fn build_rect_path(rect: Rect, radius: BorderRadius) -> Option<tiny_skia::Path> {
    let x = rect.x;
    let y = rect.y;
    let w = rect.w;
    let h = rect.h;

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

    const K: f32 = 0.5522847498;

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

impl<D, W> SoftwareRenderer<D, W>
where
    D: HasDisplayHandle,
    W: HasWindowHandle,
{
    pub(crate) fn draw_rect_impl(
        &mut self,
        rect: Rect,
        fill: Option<Color>,
        stroke: Option<Stroke>,
        radius: BorderRadius,
    ) {
        let Some(pixmap) = &mut self.pixmap else {
            return;
        };

        if let Some(color) = fill {
            if let Some(path) = build_rect_path(rect, radius) {
                let mut paint = tiny_skia::Paint::default();
                paint.set_color(to_skia_color(color));
                paint.anti_alias = true;
                pixmap.fill_path(
                    &path,
                    &paint,
                    tiny_skia::FillRule::Winding,
                    tiny_skia::Transform::identity(),
                    None,
                );
            }
        }

        if let Some(s) = stroke {
            let half = s.width / 2.0;
            let inset = Rect::new(
                rect.x + half,
                rect.y + half,
                rect.w - s.width,
                rect.h - s.width,
            );
            let inset_radius = BorderRadius {
                top_left: (radius.top_left - half).max(0.0),
                top_right: (radius.top_right - half).max(0.0),
                bottom_right: (radius.bottom_right - half).max(0.0),
                bottom_left: (radius.bottom_left - half).max(0.0),
            };
            if let Some(path) = build_rect_path(inset, inset_radius) {
                let mut paint = tiny_skia::Paint::default();
                paint.set_color(to_skia_color(s.color));
                paint.anti_alias = true;
                let stroke = tiny_skia::Stroke {
                    width: s.width,
                    ..Default::default()
                };
                pixmap.stroke_path(
                    &path,
                    &paint,
                    &stroke,
                    tiny_skia::Transform::identity(),
                    None,
                );
            }
        }
    }
}
