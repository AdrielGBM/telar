use std::collections::HashMap;
use std::sync::mpsc;

use geometry_core::Rect;
use renderer_core::{BorderRadius, RectStyle, Shadow};

use crate::primitives::fill_to_paint;
use crate::primitives::image::{ShadowCache, ShadowCacheKey};

pub(crate) fn build_rect_path(rect: Rect, radius: BorderRadius) -> Option<tiny_skia::Path> {
    let mut pb = tiny_skia::PathBuilder::new();
    push_rect_path(&mut pb, rect, radius);
    pb.finish()
}

/// Appends the box's outline as one closed subpath, or nothing at all if it has no area.
///
/// Separate from [`build_rect_path`] so a border can put its outer and inner outlines in the *same* path and
/// let the even-odd rule punch one out of the other.
fn push_rect_path(pb: &mut tiny_skia::PathBuilder, rect: Rect, radius: BorderRadius) {
    let x = rect.x;
    let y = rect.y;
    let w = rect.width;
    let h = rect.height;

    // A box with no area has no path, and neither branch below works that out for itself. `Rect::from_xywh`
    // looks like it covers the square case and does not — it refuses a negative side, not a zero one, so a
    // flat rect goes straight through it — and the rounded case clamps its radii to zero and emits a bare
    // line. Either way tiny_skia declines to fill what it is handed, and says so once per frame for as long
    // as the box stays flat.
    if !(w > 0.0 && h > 0.0) {
        return;
    }

    if radius.is_zero() {
        if let Some(r) = tiny_skia::Rect::from_xywh(x, y, w, h) {
            pb.push_rect(r);
        }
        return;
    }

    let tl = radius.top_left.min(w / 2.0).min(h / 2.0);
    let tr = radius.top_right.min(w / 2.0).min(h / 2.0);
    let br = radius.bottom_right.min(w / 2.0).min(h / 2.0);
    let bl = radius.bottom_left.min(w / 2.0).min(h / 2.0);

    let k = renderer_core::BEZIER_CIRCLE_K;

    pb.move_to(x + tl, y);
    pb.line_to(x + w - tr, y);
    pb.cubic_to(
        x + w - tr + k * tr,
        y,
        x + w,
        y + tr - k * tr,
        x + w,
        y + tr,
    );
    pb.line_to(x + w, y + h - br);
    pb.cubic_to(
        x + w,
        y + h - br + k * br,
        x + w - br + k * br,
        y + h,
        x + w - br,
        y + h,
    );
    pb.line_to(x + bl, y + h);
    pb.cubic_to(
        x + bl - k * bl,
        y + h,
        x,
        y + h - bl + k * bl,
        x,
        y + h - bl,
    );
    pb.line_to(x, y + tl);
    pb.cubic_to(x, y + tl - k * tl, x + tl - k * tl, y, x + tl, y);
    pb.close();
}

/// The border as one filled ring: the box's own outline with its inner edge punched out of it under the
/// even-odd rule.
///
/// One path rather than one stroke per side, and that is what makes the partial cases fall out instead of
/// needing to be handled. A side of zero leaves the two outlines coincident there, so the ring has no area
/// along it and covers nothing — no seam to hide, no mask to intersect. A corner where two thicknesses meet
/// tapers between them, rather than being painted twice by two strokes that overlap.
fn build_border_path(
    rect: Rect,
    radius: BorderRadius,
    widths: [f32; 4],
) -> Option<tiny_skia::Path> {
    let mut pb = tiny_skia::PathBuilder::new();
    push_rect_path(&mut pb, rect, radius);
    // No interior means the border swallowed the box, and the outer outline alone is the whole of it.
    if let Some((inner, inner_radius)) = renderer_core::border_inner_shape(rect, radius, widths) {
        push_rect_path(&mut pb, inner, inner_radius);
    }
    pb.finish()
}

#[allow(clippy::too_many_arguments)]
fn draw_rect_shadow(
    pixmap: &mut tiny_skia::Pixmap,
    rect: Rect,
    shadow: Shadow,
    radius: BorderRadius,
    transform: tiny_skia::Transform,
    clip: Option<&tiny_skia::Mask>,
    shadow_cache: &mut ShadowCache,
    pending_shadows: &mut HashMap<ShadowCacheKey, mpsc::Receiver<tiny_skia::Pixmap>>,
    recent_shadow: &mut Option<(ShadowCacheKey, u32, u32)>,
    blur_scratch: &mut Vec<u8>,
) {
    let spread = shadow.spread;

    let shadow_rect = Rect::new(
        rect.x + shadow.offset_x - spread,
        rect.y + shadow.offset_y - spread,
        rect.width + 2.0 * spread,
        rect.height + 2.0 * spread,
    );

    let padding = renderer_core::ShadowLayout::compute(
        shadow.blur_radius,
        shadow_rect.x,
        shadow_rect.x + shadow_rect.width,
        shadow_rect.y,
        shadow_rect.y + shadow_rect.height,
        1.0,
    )
    .padding;
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

    let q_blur = crate::primitives::quantize_blur(shadow.blur_radius);
    let [cr, cg, cb, ca] = shadow.color.to_rgba8();
    let color_rgba8 = u32::from_le_bytes([cr, cg, cb, ca]);
    let cache_key: crate::primitives::image::ShadowCacheKey = (
        rect.width.ceil() as u32,
        rect.height.ceil() as u32,
        shadow.spread.to_bits(),
        q_blur.to_bits(),
        color_rgba8,
        radius.top_left.to_bits(),
        radius.top_right.to_bits(),
        radius.bottom_right.to_bits(),
        radius.bottom_left.to_bits(),
    );

    let local_rect = Rect::new(
        shadow_rect.x - tmp_x as f32,
        shadow_rect.y - tmp_y as f32,
        shadow_rect.width,
        shadow_rect.height,
    );

    // The shadow shape depends only on Copy data (rect dimensions, radius, color), so the same closure body can run either inline or on a background worker thread for large shadows.
    let shadow_color = shadow.color;
    let draw_shadow = move |tmp_pmap: &mut tiny_skia::Pixmap| {
        if let Some(path) = build_rect_path(local_rect, shadow_radius) {
            let mut paint = tiny_skia::Paint::default();
            paint.set_color(crate::primitives::to_skia_color(shadow_color));
            paint.anti_alias = true;
            tmp_pmap.fill_path(
                &path,
                &paint,
                tiny_skia::FillRule::Winding,
                tiny_skia::Transform::identity(),
                None,
            );
        }
    };

    crate::primitives::blit_cached_shadow_async(
        pixmap,
        shadow_cache,
        pending_shadows,
        recent_shadow,
        cache_key,
        tmp_x,
        tmp_y,
        tmp_w,
        tmp_h,
        q_blur,
        blur_scratch,
        transform,
        clip,
        || (draw_shadow, draw_shadow),
    );
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_rect(
    pixmap: &mut tiny_skia::Pixmap,
    rect: Rect,
    style: &RectStyle,
    transform: tiny_skia::Transform,
    clip: Option<&tiny_skia::Mask>,
    shadow_cache: &mut ShadowCache,
    pending_shadows: &mut HashMap<ShadowCacheKey, mpsc::Receiver<tiny_skia::Pixmap>>,
    recent_shadow: &mut Option<(ShadowCacheKey, u32, u32)>,
    blur_scratch: &mut Vec<u8>,
) {
    if let Some(shadow) = style.shadow {
        draw_rect_shadow(
            pixmap,
            rect,
            shadow,
            style.radius,
            transform,
            clip,
            shadow_cache,
            pending_shadows,
            recent_shadow,
            blur_scratch,
        );
    }

    if let Some(fill_style) = style.fill
        && let Some(path) = build_rect_path(rect, style.radius)
    {
        let paint = fill_to_paint(fill_style);
        pixmap.fill_path(&path, &paint, tiny_skia::FillRule::Winding, transform, clip);
    }

    if let Some((border_paint, widths)) = style.border()
        && let Some(path) = build_border_path(rect, style.radius, widths)
    {
        let mut paint = fill_to_paint(border_paint);
        paint.anti_alias = true;
        pixmap.fill_path(&path, &paint, tiny_skia::FillRule::EvenOdd, transform, clip);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both branches of [`build_rect_path`] must refuse a box with no area, and neither used to.
    ///
    /// The square branch looks as though it is covered, which is the trap: `tiny_skia::Rect::from_xywh`
    /// rejects a *negative* side, not a zero one, so a flat rect passes it and reaches `push_rect` intact.
    /// The rounded branch clamps its radii to zero and draws a bare line. Both hand tiny_skia something it
    /// declines to fill and warns about, once per frame, for as long as the box stays flat.
    #[test]
    fn a_box_with_no_area_has_no_path_whatever_its_corners_are() {
        let flat = Rect {
            x: 4.0,
            y: 8.0,
            width: 120.0,
            height: 0.0,
        };
        let thin = Rect {
            width: 0.0,
            height: 120.0,
            ..flat
        };
        for rect in [flat, thin] {
            for radius in [BorderRadius::zero(), BorderRadius::all(8.0)] {
                assert!(
                    build_rect_path(rect, radius).is_none(),
                    "{rect:?} with {radius:?} still built a path"
                );
            }
        }
    }

    /// The guard is on the area alone, so a box that has one keeps its rounded path.
    #[test]
    fn a_box_with_area_still_rounds() {
        let rect = Rect {
            x: 0.0,
            y: 0.0,
            width: 64.0,
            height: 64.0,
        };
        assert!(build_rect_path(rect, BorderRadius::all(8.0)).is_some());
    }
}
