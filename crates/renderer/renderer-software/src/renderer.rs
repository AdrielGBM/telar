use std::collections::HashMap;
use std::num::NonZeroU32;

use geometry_core::Rect;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use renderer_core::{Color, DrawCommand, RenderBackend, RendererError};
use renderer_text::TextShaper;
use softbuffer::{Context, Surface};
use tiny_skia::Pixmap;

use crate::primitives::image::ImageCache;

fn repaint_mask(mask: &mut tiny_skia::Mask, rect: Rect, width: u32, height: u32) {
    mask.data_mut().fill(0);
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
    blur_scratch: Vec<u8>,
    pixmap_pool: Vec<tiny_skia::Pixmap>,
    clip_mask_buf: Option<tiny_skia::Mask>,
    draw_state: renderer_core::DrawState,
    shadow_cache: lru::LruCache<(u32, u32, u32, u32, u32), tiny_skia::Pixmap>,
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
            blur_scratch: Vec::new(),
            pixmap_pool: Vec::new(),
            clip_mask_buf: None,
            draw_state: renderer_core::DrawState::new(),
            shadow_cache: lru::LruCache::new(std::num::NonZeroUsize::new(64).unwrap()),
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
            self.clip_mask_buf = tiny_skia::Mask::new(width, height);
            self.pixmap_pool.clear();
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

    fn submit(&mut self, commands: &[DrawCommand]) -> Result<(), RendererError> {
        self.pending_commands = commands.to_vec();
        Ok(())
    }

    fn end_frame(&mut self, clear_color: Option<Color>) -> Result<(), RendererError> {
        if let (Some(color), Some(pixmap)) = (clear_color, &mut self.pixmap) {
            pixmap.fill(crate::primitives::to_skia_color(color));
        }

        let commands = std::mem::take(&mut self.pending_commands);

        self.draw_state.reset();
        let mut clip_active: bool = false;
        let mut current_clip_rect: Option<Rect> = None;
        let mut layer_stack: Vec<(tiny_skia::Pixmap, f32)> = Vec::new();

        for cmd in commands {
            if self.pixmap.is_none() {
                break;
            }
            let transform = tiny_skia::Transform::from_translate(
                self.draw_state.cum_tx,
                self.draw_state.cum_ty,
            );
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
                    let clip = if clip_active {
                        self.clip_mask_buf.as_ref()
                    } else {
                        None
                    };
                    crate::primitives::rect::draw_rect(
                        pixmap,
                        rect,
                        &style,
                        transform,
                        clip,
                        current_clip_rect,
                        &mut self.shadow_cache,
                        &mut self.blur_scratch,
                    );
                }
                DrawCommand::Text { text, rect, style } => {
                    let pixmap = if let Some((top, _)) = layer_stack.last_mut() {
                        top
                    } else {
                        self.pixmap.as_mut().unwrap()
                    };
                    let clip = if clip_active {
                        self.clip_mask_buf.as_ref()
                    } else {
                        None
                    };
                    crate::primitives::text::draw_text(
                        pixmap,
                        &mut self.text_shaper,
                        &*text,
                        rect,
                        &style,
                        transform,
                        clip,
                        &mut self.blur_scratch,
                    );
                }
                DrawCommand::Image { data, rect, filter } => {
                    let pixmap = if let Some((top, _)) = layer_stack.last_mut() {
                        top
                    } else {
                        self.pixmap.as_mut().unwrap()
                    };
                    let clip = if clip_active {
                        self.clip_mask_buf.as_ref()
                    } else {
                        None
                    };
                    crate::primitives::image::draw_image(
                        pixmap,
                        &data,
                        &mut self.image_cache,
                        rect,
                        filter,
                        transform,
                        clip,
                    );
                }
                DrawCommand::Line { p1, p2, style } => {
                    let pixmap = if let Some((top, _)) = layer_stack.last_mut() {
                        top
                    } else {
                        self.pixmap.as_mut().unwrap()
                    };
                    let clip = if clip_active {
                        self.clip_mask_buf.as_ref()
                    } else {
                        None
                    };
                    crate::primitives::line::draw_line(pixmap, p1, p2, style, transform, clip);
                }
                DrawCommand::Path { data, style } => {
                    let pixmap = if let Some((top, _)) = layer_stack.last_mut() {
                        top
                    } else {
                        self.pixmap.as_mut().unwrap()
                    };
                    let clip = if clip_active {
                        self.clip_mask_buf.as_ref()
                    } else {
                        None
                    };
                    crate::primitives::path::draw_path(
                        pixmap,
                        &data,
                        &style,
                        transform,
                        clip,
                        &mut self.blur_scratch,
                    );
                }
                DrawCommand::PushClip { rect } => {
                    let effective = self.draw_state.push_clip(rect);
                    current_clip_rect = Some(effective);
                    if let Some(ref mut m) = self.clip_mask_buf {
                        repaint_mask(m, effective, self.width, self.height);
                    }
                    clip_active = true;
                }
                DrawCommand::PopClip => {
                    let effective = self.draw_state.pop_clip();
                    match effective {
                        Some(r) => {
                            current_clip_rect = Some(r);
                            if let Some(ref mut m) = self.clip_mask_buf {
                                repaint_mask(m, r, self.width, self.height);
                            }
                            clip_active = true;
                        }
                        None => {
                            current_clip_rect = None;
                            clip_active = false;
                        }
                    }
                }
                DrawCommand::PushTransform { tx, ty } => {
                    self.draw_state.push_transform(tx, ty);
                }
                DrawCommand::PopTransform => {
                    self.draw_state.pop_transform();
                }
                DrawCommand::PushLayer { opacity } => {
                    let layer = self
                        .pixmap_pool
                        .pop()
                        .filter(|p| p.width() == self.width && p.height() == self.height)
                        .or_else(|| tiny_skia::Pixmap::new(self.width, self.height));
                    if let Some(mut l) = layer {
                        l.fill(tiny_skia::Color::TRANSPARENT);
                        layer_stack.push((l, opacity));
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
                        self.pixmap_pool.push(layer);
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
