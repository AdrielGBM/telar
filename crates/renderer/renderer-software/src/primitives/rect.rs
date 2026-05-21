use renderer_core::{BorderRadius, Rect, RectStyle, Shadow};

use crate::primitives::{fill_to_paint, to_skia_color};

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

fn draw_rect_shadow(
    pixmap: &mut tiny_skia::Pixmap,
    rect: Rect,
    shadow: Shadow,
    radius: BorderRadius,
    transform: tiny_skia::Transform,
    clip: Option<&tiny_skia::Mask>,
) {
    let sigma = shadow.blur_radius / 2.0;
    let padding = (sigma * 3.0).ceil() as i32 + 1;
    let spread = shadow.spread;

    let shadow_rect = Rect::new(
        rect.x + shadow.offset_x - spread,
        rect.y + shadow.offset_y - spread,
        rect.width + 2.0 * spread,
        rect.height + 2.0 * spread,
    );
    let shadow_radius = BorderRadius {
        top_left: (radius.top_left + spread).max(0.0),
        top_right: (radius.top_right + spread).max(0.0),
        bottom_right: (radius.bottom_right + spread).max(0.0),
        bottom_left: (radius.bottom_left + spread).max(0.0),
    };

    let tmp_x = (shadow_rect.x - padding as f32).floor() as i32;
    let tmp_y = (shadow_rect.y - padding as f32).floor() as i32;
    let tmp_w = (shadow_rect.width + 2.0 * padding as f32).ceil() as u32 + 1;
    let tmp_h = (shadow_rect.height + 2.0 * padding as f32).ceil() as u32 + 1;
    if tmp_w == 0 || tmp_h == 0 {
        return;
    }

    let Some(mut tmp) = tiny_skia::Pixmap::new(tmp_w, tmp_h) else {
        return;
    };

    let local_rect = Rect::new(
        shadow_rect.x - tmp_x as f32,
        shadow_rect.y - tmp_y as f32,
        shadow_rect.width,
        shadow_rect.height,
    );
    if let Some(path) = build_rect_path(local_rect, shadow_radius) {
        let mut paint = tiny_skia::Paint::default();
        paint.set_color(crate::primitives::to_skia_color(shadow.color));
        paint.anti_alias = true;
        tmp.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            tiny_skia::Transform::identity(),
            None,
        );
    }

    if sigma >= 0.5 {
        crate::primitives::gaussian_blur(tmp.data_mut(), tmp_w, tmp_h, sigma);
    }

    pixmap.draw_pixmap(
        tmp_x,
        tmp_y,
        tmp.as_ref(),
        &tiny_skia::PixmapPaint {
            blend_mode: tiny_skia::BlendMode::SourceOver,
            ..Default::default()
        },
        transform,
        clip,
    );
}

pub(crate) fn draw_rect(
    pixmap: &mut tiny_skia::Pixmap,
    rect: Rect,
    style: &RectStyle,
    transform: tiny_skia::Transform,
    clip: Option<&tiny_skia::Mask>,
) {
    if let Some(shadow) = style.shadow {
        draw_rect_shadow(pixmap, rect, shadow, style.radius, transform, clip);
    }

    if let Some(fill_style) = style.fill {
        if let Some(path) = build_rect_path(rect, style.radius) {
            let paint = fill_to_paint(fill_style, transform);
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
