//! Rasterizing a widget tree into pixels off-screen, for a drag preview or an exported image.

use std::cell::RefCell;
use std::fmt;
use std::mem::ManuallyDrop;

use platform_headless::HeadlessWindow;
use renderer_core::{RenderBackend, RendererError};
use renderer_software::{SoftwareRenderer, SoftwareRendererConfig};

use crate::{Color, DrawCommand};

type Rasterizer = SoftwareRenderer<HeadlessWindow, HeadlessWindow>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RasterError {
    EmptySize { width: u32, height: u32 },
    Render(String),
}

impl fmt::Display for RasterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptySize { width, height } => {
                write!(formatter, "a {width}x{height} frame has no pixels")
            }
            Self::Render(message) => write!(formatter, "render failed: {message}"),
        }
    }
}

impl std::error::Error for RasterError {}

#[derive(Debug, Clone, PartialEq)]
pub enum RedrawCheckError {
    Mismatch { frame: usize, region: crate::Rect },
    Raster(RasterError),
}

impl From<RasterError> for RedrawCheckError {
    fn from(error: RasterError) -> Self {
        Self::Raster(error)
    }
}

impl fmt::Display for RedrawCheckError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mismatch { frame, region } => write!(
                formatter,
                "frame {frame} differs from the same frame painted whole, in {region:?}"
            ),
            Self::Raster(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for RedrawCheckError {}

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
    render(commands, width, height, scale, clear_color, read_pixels).ok()
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
            .map_err(|error| RasterError::Render(error.to_string()))
    })
    .map_err(|error| error.to_string())
}

fn render<T>(
    commands: &[DrawCommand],
    width: u32,
    height: u32,
    scale: f32,
    clear_color: Option<Color>,
    read: impl FnOnce(&Rasterizer) -> Result<T, RasterError>,
) -> Result<T, RasterError> {
    ensure_pixels(width, height)?;
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
        // One generation for every call is safe only because the software rasteriser ignores it and `redraw_in_full` already drops the frame it would diff against.
        paint(renderer, commands, width, height, scale, 0, clear_color)?;
        read(renderer)
    })
}

fn ensure_pixels(width: u32, height: u32) -> Result<(), RasterError> {
    if width == 0 || height == 0 {
        return Err(RasterError::EmptySize { width, height });
    }
    Ok(())
}

fn paint<R: RenderBackend>(
    renderer: &mut R,
    commands: &[DrawCommand],
    width: u32,
    height: u32,
    scale: f32,
    generation: u64,
    clear_color: Option<Color>,
) -> Result<(), RasterError> {
    renderer
        .begin_frame(width, height, scale, generation)
        .and_then(|()| renderer.render_frame(commands, clear_color))
        .map_err(render_error)
}

/// One generation per frame of a check: a backend reads an equal generation as the same commands as the frame before, and keeps diffing the next frame against that older one.
fn frame_generation(index: usize) -> u64 {
    index as u64 + 1
}

fn read_pixels(renderer: &Rasterizer) -> Result<Vec<u8>, RasterError> {
    renderer
        .read_rgba()
        .map(<[u8]>::to_vec)
        .ok_or_else(no_picture)
}

fn render_error(error: RendererError) -> RasterError {
    RasterError::Render(error.to_string())
}

/// Paints `frames` in turn on one renderer, each over the last as a window does, and compares each with the same frame painted whole; the first that differs is the mismatch, with the box of its wrong pixels.
pub fn check_partial_redraw(
    frames: &[Vec<DrawCommand>],
    width: u32,
    height: u32,
    clear_color: Option<Color>,
) -> Result<(), RedrawCheckError> {
    ensure_pixels(width, height)?;
    let mut renderer: Rasterizer = SoftwareRenderer::new_headless(
        width,
        height,
        SoftwareRendererConfig {
            synchronous_shadows: true,
            ..SoftwareRendererConfig::default()
        },
    );
    for (index, commands) in frames.iter().enumerate() {
        let generation = frame_generation(index);
        paint(
            &mut renderer,
            commands,
            width,
            height,
            1.0,
            generation,
            clear_color,
        )?;
        let partial = read_pixels(&renderer)?;
        let full = render(commands, width, height, 1.0, clear_color, read_pixels)?;
        if let Some(region) = differing_box(&partial, &full, width) {
            return Err(RedrawCheckError::Mismatch {
                frame: index,
                region,
            });
        }
    }
    Ok(())
}

/// `NoAdapter` is a skip, not a pass: no frame was compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuRedrawCheck {
    Passed,
    NoAdapter,
}

/// [`check_partial_redraw`] on the GPU backend at `scale` device pixels per logical one, each frame over the last against the same frame on a fresh renderer; a machine with no GPU adapter answers `NoAdapter`.
#[cfg(feature = "hardware")]
pub fn check_partial_redraw_on_gpu(
    frames: &[Vec<DrawCommand>],
    width: u32,
    height: u32,
    scale: f32,
    clear_color: Option<Color>,
) -> Result<GpuRedrawCheck, RedrawCheckError> {
    let (width, height) = (
        (width as f32 * scale).round() as u32,
        (height as f32 * scale).round() as u32,
    );
    ensure_pixels(width, height)?;
    let mut partial = match new_gpu_renderer(width, height) {
        Ok(renderer) => renderer,
        Err(RendererError::NoAdapter) => return Ok(GpuRedrawCheck::NoAdapter),
        Err(error) => return Err(render_error(error).into()),
    };
    for (index, commands) in frames.iter().enumerate() {
        let generation = frame_generation(index);
        paint(
            &mut partial,
            commands,
            width,
            height,
            scale,
            generation,
            clear_color,
        )?;
        let over = read_gpu(&partial)?;
        let mut whole = new_gpu_renderer(width, height).map_err(render_error)?;
        paint(
            &mut whole,
            commands,
            width,
            height,
            scale,
            generation,
            clear_color,
        )?;
        let whole = read_gpu(&whole)?;
        if let Some(region) = differing_box(&over, &whole, width) {
            return Err(RedrawCheckError::Mismatch {
                frame: index,
                region,
            });
        }
    }
    Ok(GpuRedrawCheck::Passed)
}

#[cfg(feature = "hardware")]
fn new_gpu_renderer(
    width: u32,
    height: u32,
) -> Result<renderer_hardware::HardwareRenderer<HeadlessWindow>, renderer_core::RendererError> {
    block_on(
        renderer_hardware::HardwareRenderer::<HeadlessWindow>::new_headless(
            width,
            height,
            None,
            false,
            renderer_text::TextShaperConfig::default(),
        ),
    )
}

#[cfg(feature = "hardware")]
fn read_gpu(
    renderer: &renderer_hardware::HardwareRenderer<HeadlessWindow>,
) -> Result<Vec<u8>, RasterError> {
    renderer.read_rgba().map_err(render_error)
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

fn no_picture() -> RasterError {
    RasterError::Render(String::from("the rasterizer kept no picture"))
}

#[cfg(test)]
#[path = "raster_test.rs"]
mod tests;
