use std::num::NonZeroUsize;
use std::rc::Rc;

use clru::{CLruCache, CLruCacheConfig};
use geometry_core::Rect;
use renderer_core::{FillRule, PathData, PathStyle, PathVerb};
use rustc_hash::FxBuildHasher;

use crate::primitives::image::PixmapByteScale;
use crate::primitives::{fill_to_paint, to_skia_line_cap, to_skia_line_join};

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
    // Content hash of path vertices instead of Rc pointer: stable even when the Rc is
    // recreated each frame with the same geometry (e.g. transform-animated paths).
    path_hash: u64,
    blur_radius_bits: u32,
    spread_bits: u32,
    color: [u8; 4],
}

pub(crate) type PathShadowCache =
    CLruCache<PathShadowCacheKey, tiny_skia::Pixmap, FxBuildHasher, PixmapByteScale>;

pub(crate) fn new_path_shadow_cache(budget_bytes: usize) -> PathShadowCache {
    CLruCache::with_config(
        CLruCacheConfig::new(NonZeroUsize::new(budget_bytes.max(1)).unwrap())
            .with_hasher(FxBuildHasher::default())
            .with_scale(PixmapByteScale),
    )
}

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
    data: &Rc<PathData>,
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
) {
    let path_hash = hash_path_data(data);
    let Some(path) = build_skia_path(data) else {
        return;
    };
    let b = path.bounds();

    if let Some(shadow) = style.shadow {
        let sigma = renderer_core::blur_sigma(shadow.blur_radius);
        // blur_padding + 1 extra: paths can extend slightly outside their bounds box
        let padding = (sigma * 3.0).ceil() as i32 + 2;

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

            let q_blur = (shadow.blur_radius * 2.0).round() / 2.0;
            let [sc_r, sc_g, sc_b, sc_a] = shadow.color.to_rgba8();
            let cache_key = PathShadowCacheKey {
                path_hash,
                blur_radius_bits: q_blur.to_bits(),
                spread_bits: shadow.spread.to_bits(),
                color: [sc_r, sc_g, sc_b, sc_a],
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

            // Drawing the shadow shape only needs the path geometry, a tinted paint, and Copy style fields; this lets the work run on a background thread for large shadows. The async variant owns clones of the path and paint so the worker outlives this call.
            let draw_path_shadow =
                move |tmp_pmap: &mut tiny_skia::Pixmap,
                      path: &tiny_skia::Path,
                      shadow_paint: &tiny_skia::Paint<'static>| {
                    if has_fill {
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

            let async_path = path.clone();
            let async_paint = shadow_paint.clone();
            let draw_async = move |tmp_pmap: &mut tiny_skia::Pixmap| {
                draw_path_shadow(tmp_pmap, &async_path, &async_paint);
            };

            crate::primitives::blit_cached_shadow_async(
                pixmap,
                path_shadow_cache,
                pending_path_shadows,
                cache_key,
                draw_x,
                draw_y,
                tmp_w,
                tmp_h,
                q_blur,
                blur_scratch,
                transform,
                clip,
                |tmp_pmap| draw_path_shadow(tmp_pmap, &path, &shadow_paint),
                draw_async,
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
