use std::collections::HashMap;
use std::num::NonZeroU32;

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use renderer_core::{Color, DrawCommand, RenderBackend, RendererError};
use renderer_text::TextShaper;
use softbuffer::{Context, Surface};
use tiny_skia::Pixmap;

use crate::primitives::image::ImageCache;

pub(crate) fn to_skia_color(color: Color) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba(
        color.r.clamp(0.0, 1.0),
        color.g.clamp(0.0, 1.0),
        color.b.clamp(0.0, 1.0),
        color.a.clamp(0.0, 1.0),
    )
    .expect("channels clamped to [0,1]")
}

pub struct SoftwareRenderer<D: HasDisplayHandle, W: HasWindowHandle> {
    _context: Context<D>,
    surface: Surface<D, W>,
    width: u32,
    height: u32,
    pub(crate) pixmap: Option<Pixmap>,
    pub(crate) text_shaper: TextShaper,
    pending_commands: Vec<DrawCommand>,
    image_cache: ImageCache,
}

impl<D, W> SoftwareRenderer<D, W>
where
    D: HasDisplayHandle,
    W: HasWindowHandle,
{
    pub fn new(display: D, window: W) -> Result<Self, RendererError> {
        let context = Context::new(display).map_err(|e| {
            RendererError::Backend(format!("softbuffer context creation failed: {}", e))
        })?;
        let surface =
            Surface::new(&context, window).map_err(|e| RendererError::Surface(e.to_string()))?;
        Ok(Self {
            _context: context,
            surface,
            width: 0,
            height: 0,
            pixmap: None,
            text_shaper: TextShaper::new(),
            pending_commands: Vec::new(),
            image_cache: HashMap::new(),
        })
    }
}

impl<D, W> RenderBackend for SoftwareRenderer<D, W>
where
    D: HasDisplayHandle,
    W: HasWindowHandle,
{
    fn begin_frame(&mut self, width: u32, height: u32) -> Result<(), RendererError> {
        if width != self.width || height != self.height {
            self.width = width;
            self.height = height;
            self.pixmap = Pixmap::new(width, height);
            if let (Some(w), Some(h)) = (NonZeroU32::new(width), NonZeroU32::new(height)) {
                self.surface
                    .resize(w, h)
                    .map_err(|e| RendererError::Resize(e.to_string()))?;
            }
        }
        crate::primitives::image::evict_cache(&mut self.image_cache);
        self.pending_commands.clear();
        Ok(())
    }

    fn submit(&mut self, commands: &[DrawCommand]) {
        self.pending_commands.extend_from_slice(commands);
    }

    fn end_frame(&mut self, clear_color: Option<Color>) -> Result<(), RendererError> {
        if let (Some(color), Some(pixmap)) = (clear_color, &mut self.pixmap) {
            pixmap.fill(to_skia_color(color));
        }

        let commands = std::mem::take(&mut self.pending_commands);

        for cmd in commands {
            let Some(pixmap) = &mut self.pixmap else {
                break;
            };
            match cmd {
                DrawCommand::Rect { rect, style } => {
                    crate::primitives::rect::draw_rect(
                        pixmap,
                        rect,
                        style.fill,
                        style.stroke,
                        style.radius,
                    );
                }
                DrawCommand::Text { text, rect, style } => {
                    crate::primitives::text::draw_text(
                        pixmap,
                        &mut self.text_shaper,
                        &text,
                        rect,
                        &style,
                    );
                }
                DrawCommand::Image { data, rect, filter } => {
                    crate::primitives::image::draw_image(
                        pixmap,
                        &data,
                        &mut self.image_cache,
                        rect,
                        filter,
                    );
                }
                DrawCommand::Line { p1, p2, style } => {
                    crate::primitives::line::draw_line(pixmap, p1, p2, style);
                }
                DrawCommand::Path { data, style } => {
                    crate::primitives::path::draw_path(
                        pixmap,
                        &data,
                        style.fill,
                        style.stroke,
                        style.fill_rule,
                    );
                }
            }
        }

        let Some(pixmap) = &self.pixmap else {
            return Ok(());
        };
        if self.width == 0 || self.height == 0 {
            return Ok(());
        }
        if let Ok(mut buffer) = self.surface.buffer_mut() {
            for (dst, chunk) in buffer.iter_mut().zip(pixmap.data().chunks_exact(4)) {
                *dst = ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | chunk[2] as u32;
            }
            buffer
                .present()
                .map_err(|e| RendererError::Present(e.to_string()))?;
        }
        Ok(())
    }
}
