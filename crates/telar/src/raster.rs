//! Rasterizing a widget tree into pixels off-screen, for a drag preview or an exported image.

use std::cell::RefCell;
use std::mem::ManuallyDrop;

use platform_headless::HeadlessWindow;
use renderer_core::RenderBackend;
use renderer_software::{SoftwareRenderer, SoftwareRendererConfig};

use crate::{Color, DrawCommand};

type Rasterizer = SoftwareRenderer<HeadlessWindow, HeadlessWindow>;

reactive_core::dropless_thread_local! {
    // Freed through `reactive_core::thread_exit`, so a hot-reloaded app that rasterized can still be unloaded.
    static RASTERIZER: ManuallyDrop<RefCell<Option<Rasterizer>>> =
        const { ManuallyDrop::new(RefCell::new(None)) };
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
        if slot.is_none() {
            reactive_core::thread_exit::on_thread_exit(release);
        }
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

/// Paints `frames` one after another on one renderer, as a window does, each over what the one before left, and compares each with the same frame painted from scratch. `None` when every frame came out the same; else the first frame's index and the box of the pixels it left wrong, which is where a frame's damage missed something it changed.
///
/// A check for the renderer's damage, which repaints only the regions a frame changed and moves the pixels of a scroll rather than painting them again.
pub fn first_partial_redraw_mismatch(
    frames: &[Vec<DrawCommand>],
    width: u32,
    height: u32,
    clear_color: Option<Color>,
) -> Option<(usize, crate::Rect)> {
    let mut renderer: Rasterizer = SoftwareRenderer::new_headless(
        width,
        height,
        SoftwareRendererConfig {
            synchronous_shadows: true,
            ..SoftwareRendererConfig::default()
        },
    );
    for (index, commands) in frames.iter().enumerate() {
        renderer
            .begin_frame(width, height, 1.0, 0)
            .and_then(|()| renderer.render_frame(commands, clear_color))
            .ok()?;
        let partial = renderer.read_rgba()?.to_vec();
        let full = rasterize(commands, width, height, 1.0, clear_color)?;
        if let Some(wrong) = differing_box(&partial, &full, width) {
            return Some((index, wrong));
        }
    }
    None
}

/// [`first_partial_redraw_mismatch`] on the GPU backend at `scale` device pixels per logical one, as the backend draws logical commands onto a window that scales them: each frame painted over the last on one renderer, against the same frame painted by a renderer that has drawn nothing before it. `None` when the machine has no GPU adapter; else what the software check answers.
#[cfg(feature = "hardware")]
pub fn first_partial_redraw_mismatch_on_gpu(
    frames: &[Vec<DrawCommand>],
    width: u32,
    height: u32,
    scale: f32,
    clear_color: Option<Color>,
) -> Option<Option<(usize, crate::Rect)>> {
    use renderer_hardware::HardwareRenderer;
    let (width, height) = (
        (width as f32 * scale).round() as u32,
        (height as f32 * scale).round() as u32,
    );
    let fresh = || {
        block_on(HardwareRenderer::<HeadlessWindow>::new_headless(
            width,
            height,
            None,
            false,
            renderer_text::TextShaperConfig::default(),
        ))
        .ok()
    };
    let paint = |renderer: &mut HardwareRenderer<HeadlessWindow>, commands: &[DrawCommand]| {
        renderer
            .begin_frame(width, height, scale, 0)
            .and_then(|()| renderer.render_frame(commands, clear_color))
            .ok()?;
        renderer.read_rgba().ok()
    };
    let mut partial = fresh()?;
    for (index, commands) in frames.iter().enumerate() {
        let over = paint(&mut partial, commands)?;
        let whole = paint(&mut fresh()?, commands)?;
        if let Some(wrong) = differing_box(&over, &whole, width) {
            return Some(Some((index, wrong)));
        }
    }
    Some(None)
}

/// Runs `future` to its end on this thread, parking between polls: the GPU backend's constructor is async only to wait on the adapter.
#[cfg(feature = "hardware")]
fn block_on<F: std::future::Future>(future: F) -> F::Output {
    struct Unpark(std::thread::Thread);
    impl std::task::Wake for Unpark {
        fn wake(self: std::sync::Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = std::sync::Arc::new(Unpark(std::thread::current())).into();
    let mut context = std::task::Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        if let std::task::Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
        std::thread::park();
    }
}

/// The box of the pixels where `a` and `b`, two RGBA buffers `width` wide, differ by more than rounding.
fn differing_box(a: &[u8], b: &[u8], width: u32) -> Option<crate::Rect> {
    let mut bounds: Option<(u32, u32, u32, u32)> = None;
    for (at, (pa, pb)) in a
        .as_chunks::<4>()
        .0
        .iter()
        .zip(b.as_chunks::<4>().0)
        .enumerate()
    {
        if pa.iter().zip(pb).all(|(x, y)| x.abs_diff(*y) <= 2) {
            continue;
        }
        let (x, y) = (at as u32 % width, at as u32 / width);
        bounds = Some(match bounds {
            None => (x, y, x, y),
            Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
        });
    }
    bounds.map(|(x0, y0, x1, y1)| {
        crate::Rect::new(
            x0 as f32,
            y0 as f32,
            (x1 - x0 + 1) as f32,
            (y1 - y0 + 1) as f32,
        )
    })
}

fn release() {
    RASTERIZER.with(|cell| cell.borrow_mut().take());
}

fn no_picture() -> String {
    String::from("the rasterizer kept no picture")
}

#[cfg(test)]
#[path = "raster_test.rs"]
mod tests;
