use std::collections::HashMap;
use std::num::NonZeroU32;

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use renderer_core::{Color, DrawCommand, Rect, RenderBackend, RendererError};
use renderer_text::TextShaper;
use softbuffer::{Context, Surface};
use tiny_skia::Pixmap;

use crate::primitives::image::ImageCache;

fn build_clip_mask(rect: Rect, width: u32, height: u32) -> Option<tiny_skia::Mask> {
    let mut mask = tiny_skia::Mask::new(width, height)?;
    let x = rect.x.max(0.0);
    let y = rect.y.max(0.0);
    let right = (rect.x + rect.width).min(width as f32);
    let bottom = (rect.y + rect.height).min(height as f32);
    let w = (right - x).max(0.0);
    let h = (bottom - y).max(0.0);
    if let Some(r) = tiny_skia::Rect::from_xywh(x, y, w, h) {
        let mut pb = tiny_skia::PathBuilder::new();
        pb.push_rect(r);
        if let Some(path) = pb.finish() {
            mask.fill_path(
                &path,
                tiny_skia::FillRule::Winding,
                false,
                tiny_skia::Transform::identity(),
            );
        }
    }
    Some(mask)
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

    fn submit(&mut self, commands: Vec<DrawCommand>) -> Result<(), RendererError> {
        self.pending_commands = commands;
        Ok(())
    }

    fn end_frame(&mut self, clear_color: Option<Color>) -> Result<(), RendererError> {
        if let (Some(color), Some(pixmap)) = (clear_color, &mut self.pixmap) {
            pixmap.fill(crate::primitives::to_skia_color(color));
        }

        let commands = std::mem::take(&mut self.pending_commands);

        let mut state = renderer_core::DrawState::new();
        let mut clip_mask: Option<tiny_skia::Mask> = None;
        let mut layer_stack: Vec<(tiny_skia::Pixmap, f32)> = Vec::new();

        for cmd in commands {
            if self.pixmap.is_none() {
                break;
            }
            let transform = tiny_skia::Transform::from_translate(state.cum_tx, state.cum_ty);
            match cmd {
                DrawCommand::Rect { rect, style } => {
                    if rect.width <= 0.0
                        || rect.height <= 0.0
                        || (style.fill.is_none() && style.stroke.is_none())
                    {
                        continue;
                    }
                    let pixmap = if let Some((top, _)) = layer_stack.last_mut() {
                        top
                    } else {
                        self.pixmap.as_mut().unwrap()
                    };
                    crate::primitives::rect::draw_rect(
                        pixmap,
                        rect,
                        &style,
                        transform,
                        clip_mask.as_ref(),
                    );
                }
                DrawCommand::Text { text, rect, style } => {
                    let pixmap = if let Some((top, _)) = layer_stack.last_mut() {
                        top
                    } else {
                        self.pixmap.as_mut().unwrap()
                    };
                    crate::primitives::text::draw_text(
                        pixmap,
                        &mut self.text_shaper,
                        &*text,
                        rect,
                        &style,
                        transform,
                        clip_mask.as_ref(),
                    );
                }
                DrawCommand::Image { data, rect, filter } => {
                    let pixmap = if let Some((top, _)) = layer_stack.last_mut() {
                        top
                    } else {
                        self.pixmap.as_mut().unwrap()
                    };
                    crate::primitives::image::draw_image(
                        pixmap,
                        &data,
                        &mut self.image_cache,
                        rect,
                        filter,
                        transform,
                        clip_mask.as_ref(),
                    );
                }
                DrawCommand::Line { p1, p2, style } => {
                    let pixmap = if let Some((top, _)) = layer_stack.last_mut() {
                        top
                    } else {
                        self.pixmap.as_mut().unwrap()
                    };
                    crate::primitives::line::draw_line(
                        pixmap,
                        p1,
                        p2,
                        style,
                        transform,
                        clip_mask.as_ref(),
                    );
                }
                DrawCommand::Path { data, style } => {
                    let pixmap = if let Some((top, _)) = layer_stack.last_mut() {
                        top
                    } else {
                        self.pixmap.as_mut().unwrap()
                    };
                    crate::primitives::path::draw_path(
                        pixmap,
                        &data,
                        &style,
                        transform,
                        clip_mask.as_ref(),
                    );
                }
                DrawCommand::PushClip { rect } => {
                    let effective = state.push_clip(rect);
                    clip_mask = build_clip_mask(effective, self.width, self.height);
                }
                DrawCommand::PopClip => {
                    let effective = state.pop_clip();
                    clip_mask = effective.and_then(|r| build_clip_mask(r, self.width, self.height));
                }
                DrawCommand::PushTransform { tx, ty } => {
                    state.push_transform(tx, ty);
                }
                DrawCommand::PopTransform => {
                    state.pop_transform();
                }
                DrawCommand::PushLayer { opacity } => {
                    if let Some(layer) = tiny_skia::Pixmap::new(self.width, self.height) {
                        layer_stack.push((layer, opacity));
                    }
                }
                DrawCommand::PopLayer => {
                    if let Some((layer, opacity)) = layer_stack.pop() {
                        let target = if let Some((top, _)) = layer_stack.last_mut() {
                            top
                        } else {
                            self.pixmap.as_mut().unwrap()
                        };
                        target.draw_pixmap(
                            0,
                            0,
                            layer.as_ref(),
                            &tiny_skia::PixmapPaint {
                                opacity,
                                blend_mode: tiny_skia::BlendMode::SourceOver,
                                quality: tiny_skia::FilterQuality::Nearest,
                            },
                            tiny_skia::Transform::identity(),
                            None,
                        );
                    }
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
            // SAFETY: Pixel format conversion. tiny-skia stores pixels as RGBA in little-endian byte order: [R, G, B, A, ...]. softbuffer::Buffer accepts u32 pixels in platform-native endianness. On little-endian platforms: u32(0x00RRGGBB) → bytes [B, G, R, 0] in memory → correct. On big-endian platforms: u32(0x00RRGGBB) → bytes [0, R, G, B] in memory → incorrect. This code is only correct on little-endian.
            #[cfg(target_endian = "little")]
            {
                for (dst, chunk) in buffer.iter_mut().zip(pixmap.data().chunks_exact(4)) {
                    *dst = ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | chunk[2] as u32;
                }
            }
            #[cfg(target_endian = "big")]
            {
                compile_error!(
                    "softbuffer pixel format conversion not implemented for big-endian platforms. \
                              Please file an issue or implement proper endian-aware conversion."
                );
            }
            buffer
                .present()
                .map_err(|e| RendererError::Present(e.to_string()))?;
        }
        Ok(())
    }
}
