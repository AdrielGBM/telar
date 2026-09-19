//! Painting a bitmap into the pixmap: stretched, tiled or nine-sliced, and clipped.

use std::sync::Arc;

use geometry_core::Rect;
use renderer_cache::Cache;
use renderer_core::{ImageData, ImageFill, Raster};

/// Composite cache key for shadow pixmaps: (width, height, spread, blur_radius, color_rgba8, radius_tl, radius_tr, radius_br, radius_bl). Bits are packed as `to_bits()` for floats so equality is byte-exact.
pub(crate) type ShadowCacheKey = (u32, u32, u32, u32, u32, u32, u32, u32, u32);

pub(crate) type ShadowCache = Cache<ShadowCacheKey, tiny_skia::Pixmap>;

/// There is no cache here because `ImageData` is one: an `Arc` addressed by its own content, so copying its pixels again would only double a wallpaper's memory.
pub(crate) fn draw_image(
    pixmap: &mut tiny_skia::PixmapMut<'_>,
    data: &Arc<ImageData>,
    rect: Rect,
    filter: Raster,
    fill: ImageFill,
    transform: tiny_skia::Transform,
    clip: Option<&tiny_skia::Mask>,
) {
    let Some(source) = tiny_skia::PixmapRef::from_bytes(data.pixels(), data.width, data.height)
    else {
        return;
    };
    let quality = match filter {
        Raster::Pixel => tiny_skia::FilterQuality::Nearest,
        Raster::Smooth => tiny_skia::FilterQuality::Bilinear,
    };
    let mut painter = Painter {
        pixmap,
        source,
        quality,
        transform,
        clip,
    };
    let whole = Rect::new(0.0, 0.0, data.width as f32, data.height as f32);
    match fill {
        ImageFill::Stretch => painter.piece(whole, rect),
        ImageFill::Tile { scale } => {
            if ImageFill::tile_size(scale, data).is_some() {
                painter.tile(rect, scale);
            }
        }
        ImageFill::Slice(slice) => {
            for piece in slice.pieces((data.width, data.height), rect) {
                painter.piece(piece.source, piece.dest);
            }
        }
    }
}

struct Painter<'a, 'p> {
    pixmap: &'a mut tiny_skia::PixmapMut<'p>,
    source: tiny_skia::PixmapRef<'a>,
    quality: tiny_skia::FilterQuality,
    transform: tiny_skia::Transform,
    clip: Option<&'a tiny_skia::Mask>,
}

impl Painter<'_, '_> {
    /// `source`, in picture pixels, stretched over `dest`.
    fn piece(&mut self, source: Rect, dest: Rect) {
        let placement = tiny_skia::Transform::from_translate(-source.x, -source.y)
            .post_scale(dest.width / source.width, dest.height / source.height)
            .post_translate(dest.x, dest.y);
        self.fill(dest, tiny_skia::SpreadMode::Pad, placement);
    }

    fn tile(&mut self, dest: Rect, scale: f32) {
        let placement =
            tiny_skia::Transform::from_scale(scale, scale).post_translate(dest.x, dest.y);
        self.fill(dest, tiny_skia::SpreadMode::Repeat, placement);
    }

    // Unantialiased, as `draw_pixmap` fills: a piece's edge is a cut through the picture, and a half-covered pixel there would let the backdrop show through the seam between two pieces.
    fn fill(&mut self, dest: Rect, spread: tiny_skia::SpreadMode, placement: tiny_skia::Transform) {
        let Some(area) = tiny_skia::Rect::from_xywh(dest.x, dest.y, dest.width, dest.height) else {
            return;
        };
        let paint = tiny_skia::Paint {
            shader: tiny_skia::Pattern::new(self.source, spread, self.quality, 1.0, placement),
            anti_alias: false,
            ..Default::default()
        };
        self.pixmap
            .fill_rect(area, &paint, self.transform, self.clip);
    }
}
