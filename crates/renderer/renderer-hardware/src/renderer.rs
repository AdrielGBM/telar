use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use renderer_core::{Color, DrawCommand, ImageFilter, Rect, RenderBackend, RendererError};

use wgpu::{Device, Queue, Surface, SurfaceConfiguration, TextureViewDescriptor};

use crate::primitives::image::{ImageInstance, ImagePipeline};
use crate::primitives::line::{LineInstance, LinePipeline};
use crate::primitives::path::{PathPipeline, PathTessCache, PathVertex};
use crate::primitives::rect::{RectInstance, RectPipeline};
use crate::primitives::text::{TextInstance, TextPipeline};
use crate::primitives::{Viewport, create_viewport_bgl};

fn preferred_format(caps: &wgpu::SurfaceCapabilities) -> wgpu::TextureFormat {
    caps.formats
        .iter()
        .find(|f| {
            matches!(
                f,
                wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgba8Unorm
            )
        })
        .copied()
        .unwrap_or(caps.formats[0])
}

enum DrawStep {
    RectBatch {
        start: u32,
        end: u32,
    },
    TextBatch {
        start: u32,
        end: u32,
    },
    LineBatch {
        start: u32,
        end: u32,
    },
    ImageBatch {
        start: u32,
        end: u32,
        bind_group: wgpu::BindGroup,
    },
    PathDraw {
        index_start: u32,
        index_end: u32,
    },
    SetScissor {
        rect: Option<renderer_core::Rect>,
    },
}

#[inline]
fn flush_batch(
    pending_steps: &mut Vec<DrawStep>,
    batch_start: &mut Option<u32>,
    vec_len: u32,
    variant: impl Fn(u32, u32) -> DrawStep,
) {
    if let Some(start) = batch_start.take() {
        if vec_len > start {
            pending_steps.push(variant(start, vec_len));
        }
    }
}

#[inline]
fn flush_image_batch(
    pending_steps: &mut Vec<DrawStep>,
    batch_image_start: &mut Option<u32>,
    batch_image_bind_group: &mut Option<wgpu::BindGroup>,
    pending_image_instances_len: u32,
) {
    if let (Some(start), Some(bind_group)) =
        (batch_image_start.take(), batch_image_bind_group.take())
    {
        if pending_image_instances_len > start {
            pending_steps.push(DrawStep::ImageBatch {
                start,
                end: pending_image_instances_len,
                bind_group,
            });
        }
    }
}

/// A hardware-accelerated renderer using wgpu. The `W: Send + Sync + 'static` bound is a wgpu requirement for surface creation, not an indication that this renderer is thread-safe. The renderer must only be used on the main thread alongside the reactive runtime; it is not safe to move between threads.
pub struct HardwareRenderer<W: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static> {
    surface: Surface<'static>,
    device: Device,
    queue: Queue,
    config: Option<SurfaceConfiguration>,
    viewport_buffer: wgpu::Buffer,
    viewport_bind_group: wgpu::BindGroup,
    viewport_dirty: bool,
    rect_pipeline: RectPipeline,
    text_pipeline: TextPipeline,
    line_pipeline: LinePipeline,
    image_pipeline: ImagePipeline,
    text_shaper: renderer_text::TextShaper,
    surface_format: wgpu::TextureFormat,
    present_mode: wgpu::PresentMode,
    alpha_mode: wgpu::CompositeAlphaMode,
    width: u32,
    height: u32,
    pending_instances: Vec<RectInstance>,
    pending_text_instances: Vec<TextInstance>,
    pending_line_instances: Vec<LineInstance>,
    pending_image_instances: Vec<ImageInstance>,
    pending_steps: Vec<DrawStep>,
    path_pipeline: PathPipeline,
    pending_path_vertices: Vec<PathVertex>,
    pending_path_indices: Vec<u32>,
    path_tess_cache: PathTessCache,
    msaa_texture: Option<wgpu::Texture>,
    batch_rect_start: Option<u32>,
    batch_text_start: Option<u32>,
    batch_line_start: Option<u32>,
    batch_image_key: Option<(u64, ImageFilter)>,
    batch_image_start: Option<u32>,
    batch_image_bind_group: Option<wgpu::BindGroup>,
    _window: std::sync::Arc<W>,
}

impl<W: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static> HardwareRenderer<W> {
    pub fn new(window: W) -> Result<Self, RendererError> {
        let window = std::sync::Arc::new(window);

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });

        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| RendererError::Surface(e.to_string()))?;

        // Blocks the main thread synchronously. On slow or broken drivers this can hang indefinitely. Long-term fix: expose an async HardwareRenderer::request() + poll_ready() pair and drive it from the event loop.
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .map_err(|_| RendererError::Backend("no suitable GPU adapter found".to_string()))?;

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("rsx-hardware-renderer"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            ..Default::default()
        }))
        .map_err(|e| RendererError::Backend(format!("GPU device request failed: {}", e)))?;

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = preferred_format(&surface_caps);
        let present_mode = surface_caps
            .present_modes
            .iter()
            .find(|&&m| m == wgpu::PresentMode::Mailbox)
            .copied()
            .unwrap_or(wgpu::PresentMode::Fifo);
        let alpha_mode = surface_caps
            .alpha_modes
            .first()
            .copied()
            .unwrap_or(wgpu::CompositeAlphaMode::Auto);

        let viewport_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rsx-viewport"),
            size: std::mem::size_of::<Viewport>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let viewport_bgl = create_viewport_bgl(&device);
        let viewport_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rsx-viewport-bg"),
            layout: &viewport_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: viewport_buffer.as_entire_binding(),
            }],
        });

        let rect_pipeline = RectPipeline::new(&device, surface_format, &viewport_bgl);
        let text_pipeline = TextPipeline::new(&device, surface_format, &viewport_bgl);
        let line_pipeline = LinePipeline::new(&device, surface_format, &viewport_bgl);
        let image_pipeline = ImagePipeline::new(&device, surface_format, &viewport_bgl);
        let path_pipeline = PathPipeline::new(&device, surface_format, &viewport_bgl);
        let path_tess_cache = PathTessCache::new();

        Ok(Self {
            surface,
            device,
            queue,
            config: None,
            viewport_buffer,
            viewport_bind_group,
            viewport_dirty: true,
            rect_pipeline,
            text_pipeline,
            line_pipeline,
            image_pipeline,
            path_pipeline,
            text_shaper: renderer_text::TextShaper::new(),
            surface_format,
            present_mode,
            alpha_mode,
            width: 0,
            height: 0,
            pending_instances: Vec::new(),
            pending_text_instances: Vec::new(),
            pending_line_instances: Vec::new(),
            pending_image_instances: Vec::new(),
            pending_steps: Vec::new(),
            pending_path_vertices: Vec::new(),
            pending_path_indices: Vec::new(),
            path_tess_cache,
            msaa_texture: None,
            batch_rect_start: None,
            batch_text_start: None,
            batch_line_start: None,
            batch_image_key: None,
            batch_image_start: None,
            batch_image_bind_group: None,
            _window: window,
        })
    }

    fn reconfigure(&mut self, width: u32, height: u32) {
        let config = SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: self.surface_format,
            width,
            height,
            present_mode: self.present_mode,
            alpha_mode: self.alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        self.surface.configure(&self.device, &config);
        self.config = Some(config);
        self.viewport_dirty = true;
        self.msaa_texture = Some(self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rsx-msaa"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 4,
            dimension: wgpu::TextureDimension::D2,
            format: self.surface_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        }));
    }

    fn clear_pending(&mut self) {
        self.pending_instances.clear();
        self.pending_text_instances.clear();
        self.pending_line_instances.clear();
        self.pending_image_instances.clear();
        self.pending_steps.clear();
        self.pending_path_vertices.clear();
        self.pending_path_indices.clear();
        self.batch_rect_start = None;
        self.batch_text_start = None;
        self.batch_line_start = None;
        self.batch_image_key = None;
        self.batch_image_start = None;
        self.batch_image_bind_group = None;
    }

    fn flush_rect(&mut self) {
        flush_batch(
            &mut self.pending_steps,
            &mut self.batch_rect_start,
            self.pending_instances.len() as u32,
            |start, end| DrawStep::RectBatch { start, end },
        );
    }

    fn flush_text(&mut self) {
        flush_batch(
            &mut self.pending_steps,
            &mut self.batch_text_start,
            self.pending_text_instances.len() as u32,
            |start, end| DrawStep::TextBatch { start, end },
        );
    }

    fn flush_line(&mut self) {
        flush_batch(
            &mut self.pending_steps,
            &mut self.batch_line_start,
            self.pending_line_instances.len() as u32,
            |start, end| DrawStep::LineBatch { start, end },
        );
    }

    fn flush_image(&mut self) {
        flush_image_batch(
            &mut self.pending_steps,
            &mut self.batch_image_start,
            &mut self.batch_image_bind_group,
            self.pending_image_instances.len() as u32,
        );
    }

    fn flush_all(&mut self) {
        self.flush_rect();
        self.flush_text();
        self.flush_line();
        self.flush_image();
    }
}

impl<W: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static> RenderBackend
    for HardwareRenderer<W>
{
    fn begin_frame(&mut self, width: u32, height: u32) -> Result<(), RendererError> {
        if width != self.width || height != self.height || self.config.is_none() {
            self.width = width;
            self.height = height;
            if width > 0 && height > 0 {
                self.reconfigure(width, height);
            }
        }
        self.clear_pending();
        self.path_tess_cache.begin_frame();
        self.image_pipeline.begin_frame();
        Ok(())
    }

    fn submit(&mut self, commands: Vec<DrawCommand>) -> Result<(), RendererError> {
        let mut state = renderer_core::DrawState::new();

        for cmd in commands {
            match cmd {
                DrawCommand::Rect { rect, style } => {
                    if rect.width <= 0.0
                        || rect.height <= 0.0
                        || (style.fill.is_none() && style.stroke.is_none())
                    {
                        continue;
                    }
                    self.flush_text();
                    self.flush_line();
                    self.flush_image();
                    if self.batch_rect_start.is_none() {
                        self.batch_rect_start = Some(self.pending_instances.len() as u32);
                    }
                    let translated = Rect::new(
                        rect.x + state.cum_tx,
                        rect.y + state.cum_ty,
                        rect.width,
                        rect.height,
                    );
                    let inst = crate::primitives::rect::prepare_rect(translated, &style);
                    self.pending_instances.push(inst);
                }
                DrawCommand::Text { text, rect, style } => {
                    self.flush_rect();
                    self.flush_line();
                    self.flush_image();
                    if self.batch_text_start.is_none() {
                        self.batch_text_start = Some(self.pending_text_instances.len() as u32);
                    }
                    let translated = Rect::new(
                        rect.x + state.cum_tx,
                        rect.y + state.cum_ty,
                        rect.width,
                        rect.height,
                    );
                    let instances = crate::primitives::text::prepare_text(
                        &mut self.text_shaper,
                        &*text,
                        translated,
                        &style,
                    );
                    self.pending_text_instances.extend(instances);
                }
                DrawCommand::Line { p1, p2, style } => {
                    self.flush_rect();
                    self.flush_text();
                    self.flush_image();
                    if self.batch_line_start.is_none() {
                        self.batch_line_start = Some(self.pending_line_instances.len() as u32);
                    }
                    use renderer_core::Point;
                    let tp1 = Point::new(p1.x + state.cum_tx, p1.y + state.cum_ty);
                    let tp2 = Point::new(p2.x + state.cum_tx, p2.y + state.cum_ty);
                    self.pending_line_instances
                        .push(crate::primitives::line::prepare_line(tp1, tp2, style));
                }
                DrawCommand::Image { data, rect, filter } => {
                    self.flush_rect();
                    self.flush_text();
                    self.flush_line();
                    let key = (data.id, filter);
                    if self.batch_image_start.is_none() || self.batch_image_key != Some(key) {
                        self.flush_image();
                        self.batch_image_key = Some(key);
                        self.batch_image_start = Some(self.pending_image_instances.len() as u32);
                        self.batch_image_bind_group =
                            Some(self.image_pipeline.get_or_create_bind_group(
                                &self.device,
                                &self.queue,
                                &data,
                                filter,
                            ));
                    }
                    let translated = Rect::new(
                        rect.x + state.cum_tx,
                        rect.y + state.cum_ty,
                        rect.width,
                        rect.height,
                    );
                    self.pending_image_instances
                        .push(crate::primitives::image::prepare_image(translated));
                }
                DrawCommand::Path { data, style } => {
                    self.flush_all();
                    let vertex_start = self.pending_path_vertices.len();
                    let index_start = self.pending_path_indices.len() as u32;
                    crate::primitives::path::prepare_path(
                        &mut self.path_tess_cache,
                        &data,
                        &style,
                        &mut self.pending_path_vertices,
                        &mut self.pending_path_indices,
                    );
                    for v in &mut self.pending_path_vertices[vertex_start..] {
                        v.position[0] += state.cum_tx;
                        v.position[1] += state.cum_ty;
                    }
                    let index_end = self.pending_path_indices.len() as u32;
                    if index_end > index_start {
                        self.pending_steps.push(DrawStep::PathDraw {
                            index_start,
                            index_end,
                        });
                    }
                }
                DrawCommand::PushClip { rect } => {
                    self.flush_all();
                    let effective = state.push_clip(rect);
                    self.pending_steps.push(DrawStep::SetScissor {
                        rect: Some(effective),
                    });
                }
                DrawCommand::PopClip => {
                    self.flush_all();
                    let effective = state.pop_clip();
                    self.pending_steps
                        .push(DrawStep::SetScissor { rect: effective });
                }
                DrawCommand::PushTransform { tx, ty } => {
                    state.push_transform(tx, ty);
                }
                DrawCommand::PopTransform => {
                    state.pop_transform();
                }
            }
        }

        self.flush_all();
        Ok(())
    }

    fn end_frame(&mut self, clear_color: Option<Color>) -> Result<(), RendererError> {
        let load_op = if let Some(c) = clear_color {
            wgpu::LoadOp::Clear(wgpu::Color {
                r: c.r as f64,
                g: c.g as f64,
                b: c.b as f64,
                a: c.a as f64,
            })
        } else {
            wgpu::LoadOp::Load
        };

        if self.config.is_none() || self.width == 0 || self.height == 0 {
            self.clear_pending();
            return Ok(());
        }

        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                if let Some(config) = &self.config.clone() {
                    self.surface.configure(&self.device, config);
                }
                self.clear_pending();
                return Ok(());
            }
            other => {
                self.clear_pending();
                return Err(RendererError::Present(format!("surface error: {other:?}")));
            }
        };

        if self.viewport_dirty {
            let viewport = Viewport {
                size: [self.width as f32, self.height as f32],
                _pad: [0.0; 2],
            };
            self.queue
                .write_buffer(&self.viewport_buffer, 0, bytemuck::bytes_of(&viewport));
            self.viewport_dirty = false;
        }

        self.text_pipeline
            .sync_atlas(&self.queue, &mut self.text_shaper.atlas);

        if !self.pending_instances.is_empty() {
            self.rect_pipeline
                .instances
                .ensure_capacity(&self.device, self.pending_instances.len());
            self.queue.write_buffer(
                &self.rect_pipeline.instances.instances_buffer,
                0,
                bytemuck::cast_slice(&self.pending_instances),
            );
        }

        if !self.pending_text_instances.is_empty() {
            self.text_pipeline
                .instances
                .ensure_capacity(&self.device, self.pending_text_instances.len());
            self.queue.write_buffer(
                &self.text_pipeline.instances.instances_buffer,
                0,
                bytemuck::cast_slice(&self.pending_text_instances),
            );
        }

        if !self.pending_line_instances.is_empty() {
            self.line_pipeline
                .instances
                .ensure_capacity(&self.device, self.pending_line_instances.len());
            self.queue.write_buffer(
                &self.line_pipeline.instances.instances_buffer,
                0,
                bytemuck::cast_slice(&self.pending_line_instances),
            );
        }

        if !self.pending_image_instances.is_empty() {
            self.image_pipeline
                .instances
                .ensure_capacity(&self.device, self.pending_image_instances.len());
            self.queue.write_buffer(
                &self.image_pipeline.instances.instances_buffer,
                0,
                bytemuck::cast_slice(&self.pending_image_instances),
            );
        }

        if !self.pending_path_vertices.is_empty() {
            self.path_pipeline.ensure_capacity(
                &self.device,
                self.pending_path_vertices.len(),
                self.pending_path_indices.len(),
            );
            self.queue.write_buffer(
                &self.path_pipeline.vertex_buffer,
                0,
                bytemuck::cast_slice(&self.pending_path_vertices),
            );
            self.queue.write_buffer(
                &self.path_pipeline.index_buffer,
                0,
                bytemuck::cast_slice(&self.pending_path_indices),
            );
        }

        let view = output
            .texture
            .create_view(&TextureViewDescriptor::default());

        let msaa_view = self
            .msaa_texture
            .as_ref()
            .ok_or_else(|| {
                RendererError::Backend(
                    "msaa_texture not initialized; call reconfigure first".into(),
                )
            })?
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("rsx-encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rsx-render-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &msaa_view,
                    resolve_target: Some(&view),
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: load_op,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });

            render_pass.set_bind_group(0, &self.viewport_bind_group, &[]);

            for step in &self.pending_steps {
                match step {
                    DrawStep::RectBatch { start, end } => {
                        render_pass.set_pipeline(&self.rect_pipeline.pipeline);
                        render_pass.set_bind_group(
                            1,
                            &self.rect_pipeline.instances.instances_bind_group,
                            &[],
                        );
                        render_pass.draw(0..6, *start..*end);
                    }
                    DrawStep::TextBatch { start, end } => {
                        render_pass.set_pipeline(&self.text_pipeline.pipeline);
                        render_pass.set_bind_group(
                            1,
                            &self.text_pipeline.instances.instances_bind_group,
                            &[],
                        );
                        render_pass.set_bind_group(2, &self.text_pipeline.atlas_bind_group, &[]);
                        render_pass.draw(0..6, *start..*end);
                    }
                    DrawStep::LineBatch { start, end } => {
                        render_pass.set_pipeline(&self.line_pipeline.pipeline);
                        render_pass.set_bind_group(
                            1,
                            &self.line_pipeline.instances.instances_bind_group,
                            &[],
                        );
                        render_pass.draw(0..6, *start..*end);
                    }
                    DrawStep::ImageBatch {
                        start,
                        end,
                        bind_group,
                    } => {
                        render_pass.set_pipeline(&self.image_pipeline.pipeline);
                        render_pass.set_bind_group(
                            1,
                            &self.image_pipeline.instances.instances_bind_group,
                            &[],
                        );
                        render_pass.set_bind_group(2, bind_group, &[]);
                        render_pass.draw(0..6, *start..*end);
                    }
                    DrawStep::PathDraw {
                        index_start,
                        index_end,
                    } => {
                        render_pass.set_pipeline(&self.path_pipeline.pipeline);
                        render_pass
                            .set_vertex_buffer(0, self.path_pipeline.vertex_buffer.slice(..));
                        render_pass.set_index_buffer(
                            self.path_pipeline.index_buffer.slice(..),
                            wgpu::IndexFormat::Uint32,
                        );
                        render_pass.draw_indexed(*index_start..*index_end, 0, 0..1);
                    }
                    DrawStep::SetScissor { rect } => match rect {
                        None => {
                            render_pass.set_scissor_rect(0, 0, self.width, self.height);
                        }
                        Some(r) => {
                            let x = (r.x.max(0.0).floor() as u32).min(self.width.saturating_sub(1));
                            let y =
                                (r.y.max(0.0).floor() as u32).min(self.height.saturating_sub(1));

                            let right = ((r.x + r.width).ceil() as u32).min(self.width);
                            let bottom = ((r.y + r.height).ceil() as u32).min(self.height);
                            let w = right
                                .saturating_sub(x)
                                .max(1)
                                .min(self.width.saturating_sub(x));
                            let h = bottom
                                .saturating_sub(y)
                                .max(1)
                                .min(self.height.saturating_sub(y));
                            render_pass.set_scissor_rect(x, y, w, h);
                        }
                    },
                }
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        self.clear_pending();
        Ok(())
    }
}
