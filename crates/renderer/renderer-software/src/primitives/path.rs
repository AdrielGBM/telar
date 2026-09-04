//! Filling and stroking vector geometry, with its shadow cached against the path's content hash.

use std::sync::Arc;

use geometry_core::Rect;
use renderer_cache::Cache;
use renderer_core::{FillRule, PathData, PathStyle, PathVerb};

use crate::primitives::{fill_to_paint, to_skia_line_cap, to_skia_line_join};

/// Whether a path encloses any pixels, which is what makes filling it mean something.
///
/// A stroke of the same path is still worth drawing — that is what gives a flat chart its line — so this gates the fill alone.
fn has_area(path: &tiny_skia::Path) -> bool {
    let bounds = path.bounds();
    bounds.width() > 0.0 && bounds.height() > 0.0
}

fn hash_path_data(data: &renderer_core::PathData) -> u64 {
    use rustc_hash::FxHasher;
    use std::hash::Hasher;
    let mut h = FxHasher::default();
    for verb in data.verbs() {
        match verb {
            PathVerb::MoveTo(p) => {
                h.write_u8(0);
                h.write_u32(p.x.to_bits());
                h.write_u32(p.y.to_bits());
            }
            PathVerb::LineTo(p) => {
                h.write_u8(1);
                h.write_u32(p.x.to_bits());
                h.write_u32(p.y.to_bits());
            }
            PathVerb::QuadTo { ctrl, to } => {
                h.write_u8(2);
                h.write_u32(ctrl.x.to_bits());
                h.write_u32(ctrl.y.to_bits());
                h.write_u32(to.x.to_bits());
                h.write_u32(to.y.to_bits());
            }
            PathVerb::CubicTo { ctrl1, ctrl2, to } => {
                h.write_u8(3);
                h.write_u32(ctrl1.x.to_bits());
                h.write_u32(ctrl1.y.to_bits());
                h.write_u32(ctrl2.x.to_bits());
                h.write_u32(ctrl2.y.to_bits());
                h.write_u32(to.x.to_bits());
                h.write_u32(to.y.to_bits());
            }
            PathVerb::Close => {
                h.write_u8(4);
            }
        }
    }
    h.finish()
}

#[derive(Hash, Eq, PartialEq, Clone)]
pub(crate) struct PathShadowCacheKey {
    // A content hash rather than the `Rc` pointer: stable even when the `Rc` is recreated each frame with the same geometry.
    path_hash: u64,
    blur_radius_bits: u32,
    spread_bits: u32,
    color: [u8; 4],
    /// Whether the shadow fills, and under which rule. A path drawn only as a stroke casts a hollow shadow; the same path filled casts a solid one, and the two used to share an entry.
    has_fill: bool,
    even_odd: bool,
    /// The stroke's width, cap and join, or `None` when the path is not stroked. Width above all: a hairline and a ten-pixel stroke cast visibly different shadows from identical geometry.
    stroke: Option<(u32, u8, u8)>,
}

pub(crate) type PathShadowCache = Cache<PathShadowCacheKey, tiny_skia::Pixmap>;

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

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_path(
    pixmap: &mut tiny_skia::Pixmap,
    data: &Arc<PathData>,
    style: &PathStyle,
    transform: tiny_skia::Transform,
    clip: Option<&tiny_skia::Mask>,
    current_clip_rect: Option<Rect>,
    blur_scratch: &mut Vec<u8>,
    path_shadow_cache: &mut PathShadowCache,
    pending_path_shadows: &mut std::collections::HashMap<
        PathShadowCacheKey,
        std::sync::mpsc::Receiver<tiny_skia::Pixmap>,
    >,
    recent_path_shadow: &mut Option<(PathShadowCacheKey, u32, u32)>,
) {
    let path_hash = hash_path_data(data);
    let Some(path) = build_skia_path(data) else {
        return;
    };
    let b = path.bounds();

    if let Some(shadow) = style.shadow {
        let shadow_layout = renderer_core::ShadowLayout::compute(
            shadow.blur_radius,
            b.x() + shadow.offset_x,
            b.x() + shadow.offset_x + b.width(),
            b.y() + shadow.offset_y,
            b.y() + shadow.offset_y + b.height(),
            1.0,
        );
        let padding = shadow_layout.padding;

        let shadow_x = b.x() + shadow.offset_x - padding as f32;
        let shadow_y = b.y() + shadow.offset_y - padding as f32;
        let shadow_w = b.width() + 2.0 * padding as f32 + 4.0;
        let shadow_h = b.height() + 2.0 * padding as f32 + 4.0;
        if renderer_core::culling::overlaps(
            shadow_x + transform.tx,
            shadow_y + transform.ty,
            shadow_w,
            shadow_h,
            current_clip_rect,
        ) {
            let tmp_w = (b.width().ceil() as i32 + 2 * padding + 4).max(1) as u32;
            let tmp_h = (b.height().ceil() as i32 + 2 * padding + 4).max(1) as u32;
            let draw_x = (b.x() + shadow.offset_x) as i32 - padding;
            let draw_y = (b.y() + shadow.offset_y) as i32 - padding;

            let q_blur = crate::primitives::quantize_blur(shadow.blur_radius);
            let [sc_r, sc_g, sc_b, sc_a] = shadow.color.to_rgba8();
            let cache_key = PathShadowCacheKey {
                path_hash,
                blur_radius_bits: q_blur.to_bits(),
                spread_bits: shadow.spread.to_bits(),
                color: [sc_r, sc_g, sc_b, sc_a],
                has_fill: style.fill.is_some(),
                even_odd: style.fill_rule == FillRule::EvenOdd,
                stroke: style
                    .stroke
                    .map(|s| (s.width.to_bits(), s.cap as u8, s.join as u8)),
            };

            let dx = -b.x() + padding as f32;
            let dy = -b.y() + padding as f32;
            let shifted = tiny_skia::Transform::from_translate(dx, dy);
            let shadow_paint = {
                let mut p = tiny_skia::Paint::default();
                p.set_color(crate::primitives::to_skia_color(shadow.color));
                p.anti_alias = true;
                p
            };
            let fill_rule = style.fill_rule;
            let stroke_style = style.stroke;
            let has_fill = style.fill.is_some();

            // Drawing the shadow shape needs only the geometry, a tinted paint and Copy fields, so the work can run on a background thread. The async variant owns clones so the worker outlives this call.
            let draw_path_shadow =
                move |tmp_pmap: &mut tiny_skia::Pixmap,
                      path: &tiny_skia::Path,
                      shadow_paint: &tiny_skia::Paint<'static>| {
                    if has_fill && has_area(path) {
                        let rule = match fill_rule {
                            FillRule::Winding => tiny_skia::FillRule::Winding,
                            FillRule::EvenOdd => tiny_skia::FillRule::EvenOdd,
                        };
                        tmp_pmap.fill_path(path, shadow_paint, rule, shifted, None);
                    }
                    if let Some(s) = stroke_style {
                        let stroke = tiny_skia::Stroke {
                            width: s.width,
                            line_cap: to_skia_line_cap(s.cap),
                            line_join: to_skia_line_join(s.join),
                            ..Default::default()
                        };
                        tmp_pmap.stroke_path(path, shadow_paint, &stroke, shifted, None);
                    }
                };

            crate::primitives::blit_cached_shadow_async(
                pixmap,
                path_shadow_cache,
                pending_path_shadows,
                recent_path_shadow,
                cache_key,
                draw_x,
                draw_y,
                tmp_w,
                tmp_h,
                q_blur,
                blur_scratch,
                transform,
                clip,
                || {
                    // The worker needs its own path and paint; the inline closure can borrow the ones the fill still uses.
                    let async_path = path.clone();
                    let async_paint = shadow_paint.clone();
                    (
                        |tmp_pmap: &mut tiny_skia::Pixmap| {
                            draw_path_shadow(tmp_pmap, &path, &shadow_paint)
                        },
                        move |tmp_pmap: &mut tiny_skia::Pixmap| {
                            draw_path_shadow(tmp_pmap, &async_path, &async_paint)
                        },
                    )
                },
            );
        }
    }

    // A path with no area covers no pixel, but tiny_skia treats being asked as a mistake and warns once per frame while the shape stays flat. These paths come from data rather than a stylesheet, so flat is ordinary: a chart over equal readings, or a glyph whose outline is one straight stroke.
    if let Some(fill_style) = style.fill
        && has_area(&path)
    {
        let paint = fill_to_paint(fill_style);
        let rule = match style.fill_rule {
            FillRule::Winding => tiny_skia::FillRule::Winding,
            FillRule::EvenOdd => tiny_skia::FillRule::EvenOdd,
        };
        pixmap.fill_path(&path, &paint, rule, transform, clip);
    }

    if let Some(s) = style.stroke {
        let mut paint = fill_to_paint(s.paint);
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
