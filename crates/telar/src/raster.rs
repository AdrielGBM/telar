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
