use std::num::NonZeroU32;

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use renderer_core::{
    BorderRadius, Color, DrawCommand, FillStyle, Rect, RenderBackend, RendererError, Stroke,
    TextShaper, TextStyle,
};
use softbuffer::{Context, Surface};
use tiny_skia::Pixmap;

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
    draw_commands: Vec<DrawCommand>,
}

impl<D, W> SoftwareRenderer<D, W>
where
    D: HasDisplayHandle,
    W: HasWindowHandle,
{
    pub fn new(display: D, window: W) -> Result<Self, RendererError> {
        let context = Context::new(display).map_err(|e| RendererError::Context(e.to_string()))?;
        let surface =
            Surface::new(&context, window).map_err(|e| RendererError::Surface(e.to_string()))?;
        Ok(Self {
            _context: context,
            surface,
            width: 0,
            height: 0,
            pixmap: None,
            text_shaper: TextShaper::new(),
            draw_commands: Vec::new(),
        })
    }
}

impl<D, W> RenderBackend for SoftwareRenderer<D, W>
where
    D: HasDisplayHandle,
    W: HasWindowHandle,
{
    fn begin_frame(&mut self, width: u32, height: u32) {
        if width != self.width || height != self.height {
            self.width = width;
            self.height = height;
            self.pixmap = Pixmap::new(width, height);
            if let (Some(w), Some(h)) = (NonZeroU32::new(width), NonZeroU32::new(height)) {
                self.surface
                    .resize(w, h)
                    .expect("softbuffer surface resize failed");
            }
        }
    }

    fn draw_rect(
        &mut self,
        rect: Rect,
        fill: Option<FillStyle>,
        stroke: Option<Stroke>,
        radius: BorderRadius,
    ) {
        self.draw_commands.push(DrawCommand::Rect {
            rect,
            fill,
            stroke,
            radius,
        });
    }

    fn draw_text(&mut self, text: &str, rect: Rect, style: TextStyle) {
        self.draw_commands.push(DrawCommand::Text {
            text: text.to_owned(),
            rect,
            style,
        });
    }

    fn end_frame(&mut self, clear_color: Option<Color>) {
        if let (Some(color), Some(pixmap)) = (clear_color, &mut self.pixmap) {
            pixmap.fill(to_skia_color(color));
        }

        let commands = std::mem::take(&mut self.draw_commands);

        for cmd in commands {
            match cmd {
                DrawCommand::Rect {
                    rect,
                    fill,
                    stroke,
                    radius,
                } => {
                    self.draw_rect_impl(rect, fill, stroke, radius);
                }
                DrawCommand::Text { text, rect, style } => {
                    self.draw_text_impl(&text, rect, style.font_size, style.color);
                }
            }
        }

        let Some(pixmap) = &self.pixmap else { return };
        if self.width == 0 || self.height == 0 {
            return;
        }
        if let Ok(mut buffer) = self.surface.buffer_mut() {
            for (dst, src) in buffer.iter_mut().zip(pixmap.pixels()) {
                let r = src.red() as u32;
                let g = src.green() as u32;
                let b = src.blue() as u32;
                *dst = (r << 16) | (g << 8) | b;
            }
            buffer.present().expect("softbuffer present failed");
        }
    }
}
