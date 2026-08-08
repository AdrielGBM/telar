use std::sync::Arc;

use geometry_core::Rect;
use renderer_cache::Cache;
use renderer_core::{ImageData, ImageFilter};

pub(crate) type ImageCache = Cache<u64, tiny_skia::Pixmap>;

/// Composite cache key for shadow pixmaps: (width, height, spread, blur_radius, color_rgba8, radius_tl, radius_tr, radius_br, radius_bl). Bits are packed as `to_bits()` for floats so equality is byte-exact.
pub(crate) type ShadowCacheKey = (u32, u32, u32, u32, u32, u32, u32, u32, u32);

pub(crate) type ShadowCache = Cache<ShadowCacheKey, tiny_skia::Pixmap>;

pub(crate) fn draw_image(
    pixmap: &mut tiny_skia::Pixmap,
    data: &Arc<ImageData>,
    cache: &mut ImageCache,
    rect: Rect,
    filter: ImageFilter,
    transform: tiny_skia::Transform,
    clip: Option<&tiny_skia::Mask>,
) {
    let key = data.id;
    let quality = match filter {
        ImageFilter::Nearest => tiny_skia::FilterQuality::Nearest,
        ImageFilter::Linear => tiny_skia::FilterQuality::Bilinear,
    };
    let paint = tiny_skia::PixmapPaint {
        blend_mode: tiny_skia::BlendMode::SourceOver,
        quality,
        ..Default::default()
    };
    let image_transform = tiny_skia::Transform::from_scale(
        rect.width / data.width as f32,
        rect.height / data.height as f32,
    )
    .post_translate(rect.x, rect.y)
    .post_concat(transform);

    if let Some(cached) = cache.get(&key) {
        // A stale entry under a colliding content hash would blit the wrong pixels at the wrong stride.
        if cached.width() == data.width && cached.height() == data.height {
            pixmap.draw_pixmap(0, 0, cached.as_ref(), &paint, image_transform, clip);
        }
        return;
    }

    let Some(size) = tiny_skia::IntSize::from_wh(data.width, data.height) else {
        return;
    };
    let Some(source) = tiny_skia::Pixmap::from_vec(data.pixels.clone(), size) else {
        return;
    };
    // Drawn before it is offered, so an image the budget cannot fit still appears. The cache is a shortcut past the
    // decode, never the only route to the pixels.
    pixmap.draw_pixmap(0, 0, source.as_ref(), &paint, image_transform, clip);
    cache.insert(key, source);
}
