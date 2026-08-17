use std::cell::RefCell;

use platform_headless::HeadlessWindow;
use renderer_core::RenderBackend;
use renderer_software::{SoftwareRenderer, SoftwareRendererConfig};

use crate::{Color, DrawCommand};

thread_local! {
    // Constructing a renderer builds a text shaper and its caches, so without this every repeat caller would need its own reuse scheme to avoid paying that per call.
    static RASTERIZER: RefCell<Option<SoftwareRenderer<HeadlessWindow, HeadlessWindow>>> =
        const { RefCell::new(None) };
}

/// Draws `commands` into an offscreen buffer and hands back the pixels as premultiplied RGBA8888
/// (`[R, G, B, A]` per pixel, row-major, `width * height * 4` bytes), or `None` if the size is empty or the
/// frame failed to render.
///
/// The fourth answer alongside [`crate::try_run_test`], the preview window and
/// [`crate::run_preview_png`], and the lowest-level one: no [`crate::App`], no layout pass, no platform event
/// loop — just draw commands in, pixels out. For the caller that has already composed a few
/// [`DrawCommand`]s and needs a raw buffer to hand to something else: a compositor drag icon, a tray or
/// notification image, a texture atlas, a golden-image assertion over hand-built commands.
///
/// `commands` are expected pre-scaled, exactly as the software backend takes them on screen; `scale` is
/// reported to the backend but does not transform them. `clear_color` of `None` leaves the background fully
/// transparent.
///
/// The backing renderer is cached per thread and resized as needed, so calling this in a loop pays for the
/// text shaper once.
pub fn rasterize(
    commands: &[DrawCommand],
    width: u32,
    height: u32,
    scale: f32,
    clear_color: Option<Color>,
) -> Option<Vec<u8>> {
    if width == 0 || height == 0 {
        return None;
    }
    RASTERIZER.with(|cell| {
        let mut slot = cell.borrow_mut();
        let renderer = slot.get_or_insert_with(|| {
            SoftwareRenderer::new_headless(width, height, SoftwareRendererConfig::default())
        });
        renderer.begin_frame(width, height, scale, 0).ok()?;
        renderer.render_frame(commands, clear_color).ok()?;
        renderer.read_rgba().map(<[u8]>::to_vec)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use geometry_core::Rect;
    use renderer_core::{RectStyle, ShapeStyle};
    use std::sync::Arc;

    fn red_square(size: f32) -> Vec<DrawCommand> {
        vec![DrawCommand::Rect {
            rect: Rect::new(0.0, 0.0, size, size),
            style: Arc::new(RectStyle::default().with_fill(Color::rgba(1.0, 0.0, 0.0, 1.0))),
        }]
    }

    // hogar's drag preview and trinity's export both go through this, and neither could have told you it
    // still worked: the whole path had no test at all.
    #[test]
    fn it_draws_the_commands_it_is_given() {
        let pixels = rasterize(&red_square(8.0), 8, 8, 1.0, None).expect("pixels");
        assert_eq!(pixels.len(), 8 * 8 * 4);
        let first = &pixels[0..4];
        assert_eq!(
            (first[0], first[3]),
            (255, 255),
            "an opaque red square must reach the buffer"
        );
    }

    // `None` is documented as fully transparent, which is what a drag icon or a tray image needs: the
    // compositor blends it, so a black background would be a black box following the cursor.
    fn transparent_corner(pixels: &[u8]) -> u8 {
        pixels[pixels.len() - 1]
    }

    #[test]
    fn an_unset_clear_colour_leaves_the_background_transparent() {
        let pixels = rasterize(&red_square(4.0), 16, 16, 1.0, None).expect("pixels");
        assert_eq!(
            transparent_corner(&pixels),
            0,
            "the area the commands did not cover stays clear"
        );
    }

    // The size guard is the difference between `None` and a panic inside the backend.
    #[test]
    fn an_empty_size_is_none_rather_than_a_failure() {
        assert!(rasterize(&red_square(4.0), 0, 8, 1.0, None).is_none());
        assert!(rasterize(&red_square(4.0), 8, 0, 1.0, None).is_none());
    }

    // The renderer is cached per thread and resized as needed; a second call at a different size has to
    // produce a buffer for THAT size, not the one the cached renderer was built at.
    #[test]
    fn a_cached_renderer_still_resizes_between_calls() {
        let small = rasterize(&red_square(4.0), 8, 8, 1.0, None).expect("small");
        let large = rasterize(&red_square(4.0), 32, 24, 1.0, None).expect("large");
        assert_eq!(small.len(), 8 * 8 * 4);
        assert_eq!(large.len(), 32 * 24 * 4);
    }
}
