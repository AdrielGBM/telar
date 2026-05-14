use renderer_core::{ImageData, ImageFilter, Rect};

pub(crate) fn draw_image(
    pixmap: &mut tiny_skia::Pixmap,
    data: &ImageData,
    rect: Rect,
    filter: ImageFilter,
) {
    let Some(size) = tiny_skia::IntSize::from_wh(data.width, data.height) else {
        return;
    };

    let mut pixels = data.pixels.clone();
    super::premultiply_rgba_in_place(&mut pixels);

    let Some(src) = tiny_skia::Pixmap::from_vec(pixels, size) else {
        return;
    };

    let scale_x = rect.w / data.width as f32;
    let scale_y = rect.h / data.height as f32;

    let quality = match filter {
        ImageFilter::Nearest => tiny_skia::FilterQuality::Nearest,
        ImageFilter::Linear => tiny_skia::FilterQuality::Bilinear,
    };

    let paint = tiny_skia::PixmapPaint {
        blend_mode: tiny_skia::BlendMode::SourceOver,
        quality,
        ..Default::default()
    };

    pixmap.draw_pixmap(
        0,
        0,
        src.as_ref(),
        &paint,
        tiny_skia::Transform::from_scale(scale_x, scale_y).post_translate(rect.x, rect.y),
        None,
    );
}
