use std::collections::HashMap;
use std::rc::Rc;

use renderer_core::{ImageData, ImageFilter, Rect, premultiply_rgba};

pub(crate) type ImageCache = HashMap<u64, (Rc<ImageData>, tiny_skia::Pixmap)>;

/// Maximum simultaneous cached images. Entries beyond this limit are evicted after dead-reference cleanup to prevent unbounded memory growth.
const IMAGE_CACHE_MAX_ENTRIES: usize = 256;

/// Evicts unused cached images. The cache holds one Rc clone per entry. When strong_count == 1, no external holder remains, making the entry safe to evict. If still over `IMAGE_CACHE_MAX_ENTRIES` after that pass, arbitrary live entries are removed as a safety valve.
pub(crate) fn evict_cache(cache: &mut ImageCache) {
    cache.retain(|_, (rc, _)| Rc::strong_count(rc) > 1);
    while cache.len() > IMAGE_CACHE_MAX_ENTRIES {
        if let Some(&key) = cache.keys().next() {
            cache.remove(&key);
        } else {
            break;
        }
    }
}

pub(crate) fn draw_image(
    pixmap: &mut tiny_skia::Pixmap,
    data: &Rc<ImageData>,
    cache: &mut ImageCache,
    rect: Rect,
    filter: ImageFilter,
    transform: tiny_skia::Transform,
    clip: Option<&tiny_skia::Mask>,
) {
    let key = data.id;

    let entry = cache.entry(key).or_insert_with(|| {
        let size = tiny_skia::IntSize::from_wh(data.width, data.height);
        let src_pixmap = size.and_then(|s| {
            let mut pixels = data.pixels.clone();
            premultiply_rgba(&mut pixels);
            tiny_skia::Pixmap::from_vec(pixels, s)
        });
        let fallback = src_pixmap
            .unwrap_or_else(|| tiny_skia::Pixmap::new(1, 1).expect("1x1 pixmap always valid"));
        (Rc::clone(data), fallback)
    });

    if entry.1.width() != data.width || entry.1.height() != data.height {
        return;
    }

    let src_pixmap = &entry.1;

    let scale_x = rect.width / data.width as f32;
    let scale_y = rect.height / data.height as f32;

    let quality = match filter {
        ImageFilter::Nearest => tiny_skia::FilterQuality::Nearest,
        ImageFilter::Linear => tiny_skia::FilterQuality::Bilinear,
    };

    let paint = tiny_skia::PixmapPaint {
        blend_mode: tiny_skia::BlendMode::SourceOver,
        quality,
        ..Default::default()
    };

    let image_transform = tiny_skia::Transform::from_scale(scale_x, scale_y)
        .post_translate(rect.x, rect.y)
        .post_concat(transform);
    pixmap.draw_pixmap(0, 0, src_pixmap.as_ref(), &paint, image_transform, clip);
}
