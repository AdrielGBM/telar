use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use renderer_core::{Color, Rect};

use crate::renderer::SoftwareRenderer;

impl<D, W> SoftwareRenderer<D, W>
where
    D: HasDisplayHandle,
    W: HasWindowHandle,
{
    pub(crate) fn draw_text_impl(&mut self, text: &str, rect: Rect, font_size: f32, color: Color) {
        let (_key, pixels, tex_width, tex_height) =
            self.text_shaper.rasterize(text, rect, font_size, color);
        if tex_width == 0 || tex_height == 0 {
            return;
        }

        let Some(pixmap) = &mut self.pixmap else {
            return;
        };

        if let Some(src) = tiny_skia::Pixmap::from_vec(
            pixels,
            tiny_skia::IntSize::from_wh(tex_width, tex_height).unwrap(),
        ) {
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
}
