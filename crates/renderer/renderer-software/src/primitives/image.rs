use std::num::NonZeroUsize;
use std::sync::Arc;

use clru::{CLruCache, CLruCacheConfig, WeightScale};
use geometry_core::Rect;
use renderer_core::{ImageData, ImageFilter};
use rustc_hash::FxBuildHasher;

pub(crate) struct PixmapByteScale;

impl<K> WeightScale<K, tiny_skia::Pixmap> for PixmapByteScale {
    fn weight(&self, _key: &K, value: &tiny_skia::Pixmap) -> usize {
        value.data().len().max(1)
    }
}

pub(crate) type ImageCache = CLruCache<u64, tiny_skia::Pixmap, FxBuildHasher, PixmapByteScale>;

/// Composite cache key for shadow pixmaps: (width, height, spread, blur_radius, color_rgba8, radius_tl, radius_tr, radius_br, radius_bl). Bits are packed as `to_bits()` for floats so equality is byte-exact.
pub(crate) type ShadowCacheKey = (u32, u32, u32, u32, u32, u32, u32, u32, u32);

pub(crate) type ShadowCache =
    CLruCache<ShadowCacheKey, tiny_skia::Pixmap, FxBuildHasher, PixmapByteScale>;

pub(crate) fn new_image_cache(budget_bytes: usize) -> ImageCache {
    CLruCache::with_config(
        CLruCacheConfig::new(NonZeroUsize::new(budget_bytes).unwrap())
            .with_hasher(FxBuildHasher::default())
            .with_scale(PixmapByteScale),
    )
}

pub(crate) fn new_shadow_cache(budget_bytes: usize) -> ShadowCache {
    CLruCache::with_config(
        CLruCacheConfig::new(NonZeroUsize::new(budget_bytes.max(1)).unwrap())
            .with_hasher(FxBuildHasher::default())
            .with_scale(PixmapByteScale),
    )
}

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

    if cache.get(&key).is_none() {
        let size = tiny_skia::IntSize::from_wh(data.width, data.height);
        let src_pixmap = size.and_then(|s| tiny_skia::Pixmap::from_vec(data.pixels.clone(), s));
        let fallback = src_pixmap
            .unwrap_or_else(|| tiny_skia::Pixmap::new(1, 1).expect("1x1 pixmap always valid"));
        cache.put_with_weight(key, fallback).ok();
    }

    let Some(src_pixmap) = cache.get(&key) else {
        return;
    };

    if src_pixmap.width() != data.width || src_pixmap.height() != data.height {
        return;
    }

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
