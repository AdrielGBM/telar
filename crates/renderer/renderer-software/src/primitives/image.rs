use std::sync::Arc;

use geometry_core::Rect;
use renderer_cache::Cache;
use renderer_core::{ImageData, ImageFilter};

/// Composite cache key for shadow pixmaps: (width, height, spread, blur_radius, color_rgba8, radius_tl, radius_tr, radius_br, radius_bl). Bits are packed as `to_bits()` for floats so equality is byte-exact.
pub(crate) type ShadowCacheKey = (u32, u32, u32, u32, u32, u32, u32, u32, u32);

pub(crate) type ShadowCache = Cache<ShadowCacheKey, tiny_skia::Pixmap>;

/// Blits `data` into `rect`, straight from the pixels the caller already owns.
///
/// There is no cache here, because `ImageData` is one: an `Arc`, addressed by a hash of its own content, held
/// alive by whoever is drawing it. The cache that used to sit here stored `data.pixels.clone()` — the same bytes
/// a second time, so a wallpaper cost twice what it should, which a heap profile showed as the same ~12 MB
/// attributed once to the app and once to the renderer. Since `ImageData` is premultiplied RGBA on construction,
/// which is exactly what `PixmapRef` expects, the blit can borrow those bytes and copy nothing.
pub(crate) fn draw_image(
    pixmap: &mut tiny_skia::Pixmap,
    data: &Arc<ImageData>,
    rect: Rect,
    filter: ImageFilter,
    transform: tiny_skia::Transform,
    clip: Option<&tiny_skia::Mask>,
) {
    let Some(source) = tiny_skia::PixmapRef::from_bytes(data.pixels(), data.width, data.height)
    else {
        return;
    };
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

    pixmap.draw_pixmap(0, 0, source, &paint, image_transform, clip);
}
