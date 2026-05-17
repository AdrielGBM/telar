use std::collections::HashMap;
use std::num::NonZeroU32;

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use renderer_core::{Color, DrawCommand, Rect, RenderBackend, RendererError};
use renderer_text::TextShaper;
use softbuffer::{Context, Surface};
use tiny_skia::Pixmap;

use crate::primitives::image::ImageCache;

fn intersect_rects(a: Rect, b: Rect) -> Option<Rect> {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let right = (a.x + a.w).min(b.x + b.w);
    let bottom = (a.y + a.h).min(b.y + b.h);
    if right > x && bottom > y {
        Some(Rect::new(x, y, right - x, bottom - y))
    } else {
        None
    }
}

fn build_clip_mask(rect: Rect, width: u32, height: u32) -> Option<tiny_skia::Mask> {
    let mut mask = tiny_skia::Mask::new(width, height)?;
    let x = rect.x.max(0.0);
    let y = rect.y.max(0.0);
    let right = (rect.x + rect.w).min(width as f32);
    let bottom = (rect.y + rect.h).min(height as f32);
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

    fn submit(&mut self, commands: Vec<DrawCommand>) {
        self.pending_commands = commands;
    }

    fn end_frame(&mut self, clear_color: Option<Color>) -> Result<(), RendererError> {
        if let (Some(color), Some(pixmap)) = (clear_color, &mut self.pixmap) {
            pixmap.fill(to_skia_color(color));
        }

        let commands = std::mem::take(&mut self.pending_commands);

        let mut clip_stack: Vec<Option<Rect>> = Vec::new();
        let mut clip_mask: Option<tiny_skia::Mask> = None;
        let mut translate_stack: Vec<(f32, f32)> = Vec::new();
        let mut cum_tx: f32 = 0.0;
        let mut cum_ty: f32 = 0.0;

        for cmd in commands {
            let Some(pixmap) = &mut self.pixmap else {
                break;
            };
            let transform = tiny_skia::Transform::from_translate(cum_tx, cum_ty);
            match cmd {
                DrawCommand::Rect { rect, style } => {
                    if rect.w <= 0.0
                        || rect.h <= 0.0
                        || (style.fill.is_none() && style.stroke.is_none())
                    {
                        continue;
                    }
                    crate::primitives::rect::draw_rect(
                        pixmap,
                        rect,
                        &style,
                        transform,
                        clip_mask.as_ref(),
                    );
                }
                DrawCommand::Text { text, rect, style } => {
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
                    crate::primitives::path::draw_path(
                        pixmap,
                        &data,
                        &style,
                        transform,
                        clip_mask.as_ref(),
                    );
                }
                DrawCommand::PushClip { rect } => {
                    let effective = clip_stack
                        .last()
                        .copied()
                        .flatten()
                        .and_then(|current| intersect_rects(current, rect))
                        .or(Some(rect));
                    clip_stack.push(effective);
                    clip_mask = effective.and_then(|r| build_clip_mask(r, self.width, self.height));
                }
                DrawCommand::PopClip => {
                    clip_stack.pop();
                    let effective = clip_stack.last().copied().flatten();
                    clip_mask = effective.and_then(|r| build_clip_mask(r, self.width, self.height));
                }
                DrawCommand::PushTransform { tx, ty } => {
                    translate_stack.push((tx, ty));
                    cum_tx += tx;
                    cum_ty += ty;
                }
                DrawCommand::PopTransform => {
                    if let Some((tx, ty)) = translate_stack.pop() {
                        cum_tx -= tx;
                        cum_ty -= ty;
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
