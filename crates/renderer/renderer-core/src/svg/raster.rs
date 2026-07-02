use std::sync::Arc;

use geometry_core::Rect;

use crate::{Color, DrawCommand, ImageData, ImageFilter};

use super::SvgData;

impl SvgData {
    pub(super) fn raster_fallback(
        &self,
        fitted_w: f32,
        fitted_h: f32,
        offset_x: f32,
        offset_y: f32,
        tint: Option<Color>,
    ) -> Vec<DrawCommand> {
        let (vb_w, vb_h) = self.size;
        // Render at 2x: this layer does not know the display scale factor, so 2x keeps icons crisp on HiDPI.
        const DENSITY: f32 = 2.0;
        const MAX_SIDE: f32 = 4096.0;
        let mut px_w = (fitted_w * DENSITY).ceil();
        let mut px_h = (fitted_h * DENSITY).ceil();
        let max_side = px_w.max(px_h);
        if max_side > MAX_SIDE {
            let k = MAX_SIDE / max_side;
            px_w = (px_w * k).floor();
            px_h = (px_h * k).floor();
        }
        let pw = (px_w as u32).max(1);
        let ph = (px_h as u32).max(1);

        let Some(mut pixmap) = resvg::tiny_skia::Pixmap::new(pw, ph) else {
            return Vec::new();
        };
        // The pixmap covers exactly the letterboxed content rect, so map the whole intrinsic viewBox onto it.
        let render_ts = resvg::tiny_skia::Transform::from_scale(pw as f32 / vb_w, ph as f32 / vb_h);
        resvg::render(&self.tree, render_ts, &mut pixmap.as_mut());

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
}

fn apply_tint_premultiplied(pixels: &mut [u8], tint: Color) {
    for px in pixels.chunks_exact_mut(4) {
        // Buffer is premultiplied, so its alpha byte already equals the source coverage.
        let coverage = px[3] as f32 / 255.0;
        let out_a = coverage * tint.a;
        px[0] = (tint.r * out_a * 255.0).round().clamp(0.0, 255.0) as u8;
        px[1] = (tint.g * out_a * 255.0).round().clamp(0.0, 255.0) as u8;
        px[2] = (tint.b * out_a * 255.0).round().clamp(0.0, 255.0) as u8;
        px[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
    }
}
