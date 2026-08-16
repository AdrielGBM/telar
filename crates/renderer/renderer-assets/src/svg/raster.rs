use std::sync::Arc;

use geometry_core::Rect;

use crate::image::apply_tint_premultiplied;
use renderer_core::{Color, DrawCommand, ImageData, ImageFilter};

/// Pixel dimensions to rasterize a `w`x`h` region into, shared by the runtime fallback and the baker so the two stay comparably crisp. Renders at 2x because this layer does not know the display scale factor, then clamps the long side so a large viewBox cannot ask for an unallocatable pixmap.
pub(super) fn raster_px(w: f32, h: f32) -> (u32, u32) {
    const DENSITY: f32 = 2.0;
    const MAX_SIDE: f32 = 4096.0;
    let mut px_w = (w * DENSITY).ceil();
    let mut px_h = (h * DENSITY).ceil();
    let max_side = px_w.max(px_h);
    if max_side > MAX_SIDE {
        let k = MAX_SIDE / max_side;
        px_w = (px_w * k).floor();
        px_h = (px_h * k).floor();
    }
    ((px_w as u32).max(1), (px_h as u32).max(1))
}

/// Rasterizes the whole tree into the letterboxed content rect for SVG features we have no vector primitive for.
pub(super) fn raster_fallback(
    tree: &usvg::Tree,
    size: (f32, f32),
    fitted_w: f32,
    fitted_h: f32,
    offset_x: f32,
    offset_y: f32,
    tint: Option<Color>,
) -> Vec<DrawCommand> {
    let (vb_w, vb_h) = size;
    let (pw, ph) = raster_px(fitted_w, fitted_h);

    let Some(mut pixmap) = resvg::tiny_skia::Pixmap::new(pw, ph) else {
        return Vec::new();
    };
    // The pixmap covers exactly the letterboxed content rect, so map the whole intrinsic viewBox onto it.
    let render_ts = resvg::tiny_skia::Transform::from_scale(pw as f32 / vb_w, ph as f32 / vb_h);
    resvg::render(tree, render_ts, &mut pixmap.as_mut());

    let mut pixels = pixmap.take();
    if let Some(tint) = tint {
        apply_tint_premultiplied(&mut pixels, tint);
    }
    // resvg's Pixmap is already premultiplied RGBA8; use the constructor that skips premultiplication.
    let data = ImageData::from_premultiplied(pixels, pw, ph);
    vec![DrawCommand::Image {
        data: Arc::new(data),
        rect: Rect::new(offset_x, offset_y, fitted_w, fitted_h),
        filter: ImageFilter::Linear,
    }]
}
