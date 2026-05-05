use std::num::NonZeroU32;

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use renderer_core::{Color, RenderBackend};
use softbuffer::{Context, Surface};

pub struct SoftwareRenderer<D: HasDisplayHandle, W: HasWindowHandle> {
    _context: Context<D>,
    surface: Surface<D, W>,
    width: u32,
    height: u32,
    pixels: Vec<u32>,
}

impl<D, W> SoftwareRenderer<D, W>
where
    D: HasDisplayHandle,
    W: HasWindowHandle,
{
    pub fn new(display: D, window: W) -> Self {
        let context = Context::new(display).unwrap();
        let surface = Surface::new(&context, window).unwrap();
        Self {
            _context: context,
            surface,
            width: 0,
            height: 0,
            pixels: Vec::new(),
        }
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
            self.pixels.resize((width * height) as usize, 0);
            if let (Some(w), Some(h)) = (NonZeroU32::new(width), NonZeroU32::new(height)) {
                self.surface.resize(w, h).unwrap();
            }
        }
    }

    fn clear(&mut self, color: Color) {
        self.pixels.fill(color_to_xrgb(color));
    }

    fn end_frame(&mut self) {
        if self.width == 0 || self.height == 0 {
            return;
        }
        if let Ok(mut buffer) = self.surface.buffer_mut() {
            buffer.copy_from_slice(&self.pixels);
            buffer.present().unwrap();
        }
    }
}

fn color_to_xrgb(color: Color) -> u32 {
    let r = (color.r.clamp(0.0, 1.0) * 255.0) as u32;
    let g = (color.g.clamp(0.0, 1.0) * 255.0) as u32;
    let b = (color.b.clamp(0.0, 1.0) * 255.0) as u32;
    (r << 16) | (g << 8) | b
}
