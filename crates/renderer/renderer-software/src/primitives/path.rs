use renderer_core::{FillRule, PathData, PathStyle, PathVerb};

use crate::primitives::{fill_to_paint, to_skia_color, to_skia_line_cap, to_skia_line_join};

fn build_skia_path(data: &PathData) -> Option<tiny_skia::Path> {
    let mut pb = tiny_skia::PathBuilder::new();
    for verb in data.verbs() {
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

pub(crate) fn draw_path(
    pixmap: &mut tiny_skia::Pixmap,
    data: &PathData,
    style: &PathStyle,
    transform: tiny_skia::Transform,
    clip: Option<&tiny_skia::Mask>,
    blur_scratch: &mut Vec<u8>,
) {
    let Some(path) = build_skia_path(data) else {
        return;
    };

    if let Some(shadow) = style.shadow {
        let b = path.bounds();
        let sigma = shadow.blur_radius / 2.0;
        let padding = (sigma * 3.0).ceil() as i32 + 2;
        let tmp_w = (b.width().ceil() as i32 + 2 * padding + 4).max(1) as u32;
        let tmp_h = (b.height().ceil() as i32 + 2 * padding + 4).max(1) as u32;
        if let Some(mut tmp) = tiny_skia::Pixmap::new(tmp_w, tmp_h) {
            let dx = -b.x() + padding as f32;
            let dy = -b.y() + padding as f32;
            let shifted = tiny_skia::Transform::from_translate(dx, dy);
            let shadow_paint = {
                let mut p = tiny_skia::Paint::default();
                p.set_color(crate::primitives::to_skia_color(shadow.color));
                p.anti_alias = true;
                p
            };
            if style.fill.is_some() {
                let rule = match style.fill_rule {
                    FillRule::Winding => tiny_skia::FillRule::Winding,
                    FillRule::EvenOdd => tiny_skia::FillRule::EvenOdd,
                };
                tmp.fill_path(&path, &shadow_paint, rule, shifted, None);
            }
            if let Some(s) = style.stroke {
                let stroke = tiny_skia::Stroke {
                    width: s.width,
                    line_cap: to_skia_line_cap(s.cap),
                    line_join: to_skia_line_join(s.join),
                    ..Default::default()
                };
                tmp.stroke_path(&path, &shadow_paint, &stroke, shifted, None);
            }
            if sigma >= 0.5 {
                crate::primitives::gaussian_blur(tmp.data_mut(), tmp_w, tmp_h, sigma, blur_scratch);
            }
            let shadow_offset_x = shadow.offset_x;
            let shadow_offset_y = shadow.offset_y;
            let draw_x = (b.x() + shadow_offset_x) as i32 - padding;
            let draw_y = (b.y() + shadow_offset_y) as i32 - padding;
            pixmap.draw_pixmap(
                draw_x,
                draw_y,
                tmp.as_ref(),
                &tiny_skia::PixmapPaint {
                    blend_mode: tiny_skia::BlendMode::SourceOver,
                    ..Default::default()
                },
                transform,
                clip,
            );
        }
    }

    if let Some(fill_style) = style.fill {
        let paint = fill_to_paint(fill_style);
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
