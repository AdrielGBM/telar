//! Rasterizing a widget tree into pixels off-screen, for a drag preview or an exported image.

use std::cell::RefCell;

use platform_headless::HeadlessWindow;
use renderer_core::RenderBackend;
use renderer_software::{SoftwareRenderer, SoftwareRendererConfig};

use crate::{Color, DrawCommand};

type Rasterizer = SoftwareRenderer<HeadlessWindow, HeadlessWindow>;

thread_local! {
    static RASTERIZER: RefCell<Option<Rasterizer>> = const { RefCell::new(None) };
}

/// Draws `commands` into an offscreen buffer and hands back the pixels as premultiplied RGBA8888 (`[R, G, B, A]` per pixel, row-major, `width * height * 4` bytes), or `None` if the size is empty or the frame failed to render.
///
/// The fourth answer alongside [`crate::try_run_test`], the preview window and [`crate::run_preview_png`], and the lowest-level one: no [`crate::App`], no layout pass, no platform event loop — just draw commands in, pixels out. For the caller that has already composed a few [`DrawCommand`]s and needs a raw buffer to hand to something else: a compositor drag icon, a tray or notification image, a texture atlas, a golden-image assertion over hand-built commands.
///
/// `commands` are expected pre-scaled, exactly as the software backend takes them on screen; `scale` is reported to the backend but does not transform them. `clear_color` of `None` leaves the background fully transparent.
///
/// The backing renderer is cached per thread and resized as needed, and the text shaper and shadow caches it draws with are the thread's own, so calling this in a loop allocates neither again. Every call is a full redraw with its shadows blurred inline, so a frame never inherits pixels or shadows from the one before it.
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
    render(commands, width, height, scale, clear_color, |renderer| {
        renderer
            .read_rgba()
            .map(<[u8]>::to_vec)
            .ok_or_else(no_picture)
    })
    .ok()
}

/// [`rasterize`] at one pixel per logical pixel, encoded as a PNG.
#[cfg(any(feature = "preview-headless", feature = "preview-snapshots"))]
pub(crate) fn rasterize_png(
    commands: &[DrawCommand],
    width: u32,
    height: u32,
    clear_color: Option<Color>,
) -> Result<Vec<u8>, String> {
    render(commands, width, height, 1.0, clear_color, |renderer| {
        renderer
            .pixmap()
            .ok_or_else(no_picture)?
            .encode_png()
            .map_err(|error| error.to_string())
    })
}

fn render<T>(
    commands: &[DrawCommand],
    width: u32,
    height: u32,
    scale: f32,
    clear_color: Option<Color>,
    read: impl FnOnce(&Rasterizer) -> Result<T, String>,
) -> Result<T, String> {
    RASTERIZER.with(|cell| {
        let mut slot = cell.borrow_mut();
        let renderer = slot.get_or_insert_with(|| {
            SoftwareRenderer::new_headless(
                width,
                height,
                SoftwareRendererConfig {
                    synchronous_shadows: true,
                    ..SoftwareRendererConfig::default()
                },
            )
        });
        renderer.redraw_in_full();
        renderer
            .begin_frame(width, height, scale, 0)
            .and_then(|()| renderer.render_frame(commands, clear_color))
            .map_err(|error| error.to_string())?;
        read(renderer)
    })
}

fn no_picture() -> String {
    String::from("the rasterizer kept no picture")
}

#[cfg(test)]
#[path = "raster_test.rs"]
mod tests;
