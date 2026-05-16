use renderer_core::{Rect, TextStyle};

pub(crate) fn draw_text(
    pixmap: &mut tiny_skia::Pixmap,
    shaper: &mut renderer_text::TextShaper,
    text: &str,
    rect: Rect,
    style: &TextStyle,
) {
    let (_key, pixels, tex_width, tex_height) = shaper.rasterize(text, rect, style);
    if tex_width == 0 || tex_height == 0 {
        return;
    }

    let Some(size) = tiny_skia::IntSize::from_wh(tex_width, tex_height) else {
        return;
    };

    if let Some(src) = tiny_skia::Pixmap::from_vec(pixels, size) {
        let paint = tiny_skia::PixmapPaint {
            blend_mode: tiny_skia::BlendMode::SourceOver,
            ..Default::default()
        };
        pixmap.draw_pixmap(
            rect.x as i32,
            rect.y as i32,
            src.as_ref(),
            &paint,
            tiny_skia::Transform::identity(),
            None,
        );
    }
}
