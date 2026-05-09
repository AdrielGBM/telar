use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use renderer_core::{Rect, TextStyle};

use crate::renderer::SoftwareRenderer;

impl<D, W> SoftwareRenderer<D, W>
where
    D: HasDisplayHandle,
    W: HasWindowHandle,
{
    pub(crate) fn draw_text_impl(&mut self, text: &str, rect: Rect, style: &TextStyle) {
        let (_key, mut pixels, tex_width, tex_height) =
            self.text_shaper.rasterize(text, rect, style);
        if tex_width == 0 || tex_height == 0 {
            return;
        }

        let Some(pixmap) = &mut self.pixmap else {
            return;
        };

        let Some(size) = tiny_skia::IntSize::from_wh(tex_width, tex_height) else {
            return;
        };

        for chunk in pixels.chunks_exact_mut(4) {
            let a = chunk[3] as u32;
            chunk[0] = ((chunk[0] as u32 * a) / 255) as u8;
            chunk[1] = ((chunk[1] as u32 * a) / 255) as u8;
            chunk[2] = ((chunk[2] as u32 * a) / 255) as u8;
        }

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
}
