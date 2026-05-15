use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use renderer_core::{Color, DrawCommand, RenderBackend, RendererError};
use wgpu::{Device, Queue, Surface, SurfaceConfiguration, TextureViewDescriptor};

use crate::primitives::Viewport;
use crate::primitives::image::{ImageInstance, ImagePipeline, make_image_instance};
use crate::primitives::line::{LineInstance, LinePipeline, make_line_instance};
use crate::primitives::path::{PathPipeline, PathTessCache, PathVertex, tessellate_path};
use crate::primitives::rect::{RectInstance, RectPipeline};
use crate::primitives::text::{TextInstance, TextPipeline};

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
    ImageDraw {
        instance_index: u32,
        texture_bind_group: wgpu::BindGroup,
    },
    PathDraw {
        index_start: u32,
        index_end: u32,
    },
}

pub struct HardwareRenderer<W: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static> {
    surface: Surface<'static>,
    device: Device,
    queue: Queue,
    config: Option<SurfaceConfiguration>,
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

        let rect_pipeline = RectPipeline::new(&device, surface_format);
        let text_pipeline = TextPipeline::new(&device, surface_format);
        let line_pipeline = LinePipeline::new(&device, surface_format);
        let image_pipeline = ImagePipeline::new(&device, surface_format);
        let path_pipeline = PathPipeline::new(&device, surface_format);
        let path_tess_cache = PathTessCache::new();

        Ok(Self {
            surface,
            device,
            queue,
            config: None,
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
        self.pending_instances.clear();
        self.pending_text_instances.clear();
        self.pending_line_instances.clear();
        self.pending_image_instances.clear();
        self.pending_steps.clear();
        self.path_tess_cache.begin_frame();
        self.pending_path_vertices.clear();
        self.pending_path_indices.clear();
        Ok(())
    }

    fn submit(&mut self, commands: &[DrawCommand]) {
        self.image_pipeline.begin_frame();

        let mut current_rect_start: Option<u32> = None;
        let mut current_text_start: Option<u32> = None;
        let mut current_line_start: Option<u32> = None;

        macro_rules! flush_rect {
            () => {
                if let Some(start) = current_rect_start.take() {
                    let end = self.pending_instances.len() as u32;
                    if end > start {
                        self.pending_steps.push(DrawStep::RectBatch { start, end });
                    }
                }
            };
        }
        macro_rules! flush_text {
            () => {
                if let Some(start) = current_text_start.take() {
                    let end = self.pending_text_instances.len() as u32;
                    if end > start {
                        self.pending_steps.push(DrawStep::TextBatch { start, end });
                    }
                }
            };
        }
        macro_rules! flush_line {
            () => {
                if let Some(start) = current_line_start.take() {
                    let end = self.pending_line_instances.len() as u32;
                    if end > start {
                        self.pending_steps.push(DrawStep::LineBatch { start, end });
                    }
                }
            };
        }

        for cmd in commands {
            match cmd {
                DrawCommand::Rect {
                    rect,
                    fill,
                    stroke,
                    radius,
                } => {
                    flush_text!();
                    flush_line!();
                    if current_rect_start.is_none() {
                        current_rect_start = Some(self.pending_instances.len() as u32);
                    }
                    let inst =
                        crate::primitives::rect::make_rect_instance(*rect, *fill, *stroke, *radius);
                    self.pending_instances.push(inst);
                }
                DrawCommand::Text { text, rect, style } => {
                    flush_rect!();
                    flush_line!();
                    if current_text_start.is_none() {
                        current_text_start = Some(self.pending_text_instances.len() as u32);
                    }
                    let glyphs = self.text_shaper.layout_glyphs(text, *rect, style);
                    self.pending_text_instances
                        .extend(glyphs.iter().map(|g| TextInstance {
                            dest_rect: g.dest_rect,
                            uv_min: g.uv_min,
                            uv_max: g.uv_max,
                        }));
                }
                DrawCommand::Line { p1, p2, style } => {
                    flush_rect!();
                    flush_text!();
                    if current_line_start.is_none() {
                        current_line_start = Some(self.pending_line_instances.len() as u32);
                    }
                    self.pending_line_instances
                        .push(make_line_instance(*p1, *p2, *style));
                }
                DrawCommand::Image { data, rect, filter } => {
                    flush_rect!();
                    flush_text!();
                    flush_line!();
                    let instance_index = self.pending_image_instances.len() as u32;
                    self.pending_image_instances
                        .push(make_image_instance(*rect));
                    let texture_bind_group = self.image_pipeline.get_or_create_bind_group(
                        &self.device,
                        &self.queue,
                        data,
                        *filter,
                    );
                    self.pending_steps.push(DrawStep::ImageDraw {
                        instance_index,
                        texture_bind_group,
                    });
                }
                DrawCommand::Path {
                    data,
                    fill,
                    stroke,
                    fill_rule,
                } => {
                    flush_rect!();
                    flush_text!();
                    flush_line!();
                    let index_start = self.pending_path_indices.len() as u32;
                    tessellate_path(
                        &mut self.path_tess_cache,
                        data,
                        *fill,
                        *stroke,
                        *fill_rule,
                        &mut self.pending_path_vertices,
                        &mut self.pending_path_indices,
                    );
                    let index_end = self.pending_path_indices.len() as u32;
                    if index_end > index_start {
                        self.pending_steps.push(DrawStep::PathDraw {
                            index_start,
                            index_end,
                        });
                    }
                }
                _ => {}
            }
        }

        flush_rect!();
        flush_text!();
        flush_line!();
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
            self.pending_instances.clear();
            self.pending_text_instances.clear();
            self.pending_line_instances.clear();
            self.pending_image_instances.clear();
            self.pending_steps.clear();
            self.pending_path_vertices.clear();
            self.pending_path_indices.clear();
            return Ok(());
        }

        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                if let Some(config) = &self.config.clone() {
                    self.surface.configure(&self.device, config);
                }
                self.pending_instances.clear();
                self.pending_text_instances.clear();
                self.pending_line_instances.clear();
                self.pending_image_instances.clear();
                self.pending_steps.clear();
                self.pending_path_vertices.clear();
                self.pending_path_indices.clear();
                return Ok(());
            }
            other => {
                self.pending_instances.clear();
                self.pending_text_instances.clear();
                self.pending_line_instances.clear();
                self.pending_image_instances.clear();
                self.pending_steps.clear();
                self.pending_path_vertices.clear();
                self.pending_path_indices.clear();
                return Err(RendererError::Present(format!("surface error: {other:?}")));
            }
        };

        let viewport = Viewport {
            size: [self.width as f32, self.height as f32],
            _pad: [0.0; 2],
        };
        self.queue.write_buffer(
            &self.rect_pipeline.instances.viewport_buffer,
            0,
            bytemuck::bytes_of(&viewport),
        );
        self.queue.write_buffer(
            &self.text_pipeline.instances.viewport_buffer,
            0,
            bytemuck::bytes_of(&viewport),
        );
        self.queue.write_buffer(
            &self.line_pipeline.instances.viewport_buffer,
            0,
            bytemuck::bytes_of(&viewport),
        );
        self.queue.write_buffer(
            &self.image_pipeline.instances.viewport_buffer,
            0,
            bytemuck::bytes_of(&viewport),
        );
        self.queue.write_buffer(
            &self.path_pipeline.viewport_buffer,
            0,
            bytemuck::bytes_of(&viewport),
        );

        self.text_pipeline
            .sync_atlas(&self.queue, &mut self.text_shaper.atlas);

        let all_instances = std::mem::take(&mut self.pending_instances);
        let text_instances = std::mem::take(&mut self.pending_text_instances);
        let line_instances = std::mem::take(&mut self.pending_line_instances);
        let image_instances = std::mem::take(&mut self.pending_image_instances);
        let steps = std::mem::take(&mut self.pending_steps);
        let path_vertices = std::mem::take(&mut self.pending_path_vertices);
        let path_indices = std::mem::take(&mut self.pending_path_indices);

        if !all_instances.is_empty() {
            self.rect_pipeline
                .ensure_capacity(&self.device, all_instances.len());
            self.queue.write_buffer(
                &self.rect_pipeline.instances.instances_buffer,
                0,
                bytemuck::cast_slice(&all_instances),
            );
        }

        if !text_instances.is_empty() {
            self.text_pipeline
                .ensure_capacity(&self.device, text_instances.len());
            self.queue.write_buffer(
                &self.text_pipeline.instances.instances_buffer,
                0,
                bytemuck::cast_slice(&text_instances),
            );
        }

        if !line_instances.is_empty() {
            self.line_pipeline
                .ensure_capacity(&self.device, line_instances.len());
            self.queue.write_buffer(
                &self.line_pipeline.instances.instances_buffer,
                0,
                bytemuck::cast_slice(&line_instances),
            );
        }

        if !image_instances.is_empty() {
            self.image_pipeline
                .ensure_capacity(&self.device, image_instances.len());
            self.queue.write_buffer(
                &self.image_pipeline.instances.instances_buffer,
                0,
                bytemuck::cast_slice(&image_instances),
            );
        }

        if !path_vertices.is_empty() {
            self.path_pipeline.ensure_capacity(
                &self.device,
                path_vertices.len(),
                path_indices.len(),
            );
            self.queue.write_buffer(
                &self.path_pipeline.vertex_buffer,
                0,
                bytemuck::cast_slice(&path_vertices),
            );
            self.queue.write_buffer(
                &self.path_pipeline.index_buffer,
                0,
                bytemuck::cast_slice(&path_indices),
            );
        }

        let view = output
            .texture
            .create_view(&TextureViewDescriptor::default());

        let msaa_view = self
            .msaa_texture
            .as_ref()
            .expect("msaa_texture initialized in reconfigure")
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

            for step in &steps {
                match step {
                    DrawStep::RectBatch { start, end } => {
                        render_pass.set_pipeline(&self.rect_pipeline.pipeline);
                        render_pass.set_bind_group(
                            0,
                            &self.rect_pipeline.instances.instances_bind_group,
                            &[],
                        );
                        render_pass.draw(0..6, *start..*end);
                    }
                    DrawStep::TextBatch { start, end } => {
                        render_pass.set_pipeline(&self.text_pipeline.pipeline);
                        render_pass.set_bind_group(
                            0,
                            &self.text_pipeline.instances.instances_bind_group,
                            &[],
                        );
                        render_pass.set_bind_group(1, &self.text_pipeline.atlas_bind_group, &[]);
                        render_pass.draw(0..6, *start..*end);
                    }
                    DrawStep::LineBatch { start, end } => {
                        render_pass.set_pipeline(&self.line_pipeline.pipeline);
                        render_pass.set_bind_group(
                            0,
                            &self.line_pipeline.instances.instances_bind_group,
                            &[],
                        );
                        render_pass.draw(0..6, *start..*end);
                    }
                    DrawStep::ImageDraw {
                        instance_index,
                        texture_bind_group,
                    } => {
                        render_pass.set_pipeline(&self.image_pipeline.pipeline);
                        render_pass.set_bind_group(
                            0,
                            &self.image_pipeline.instances.instances_bind_group,
                            &[],
                        );
                        render_pass.set_bind_group(1, texture_bind_group, &[]);
                        render_pass.draw(0..6, *instance_index..*instance_index + 1);
                    }
                    DrawStep::PathDraw {
                        index_start,
                        index_end,
                    } => {
                        render_pass.set_pipeline(&self.path_pipeline.pipeline);
                        render_pass.set_bind_group(0, &self.path_pipeline.bind_group, &[]);
                        render_pass
                            .set_vertex_buffer(0, self.path_pipeline.vertex_buffer.slice(..));
                        render_pass.set_index_buffer(
                            self.path_pipeline.index_buffer.slice(..),
                            wgpu::IndexFormat::Uint32,
                        );
                        render_pass.draw_indexed(*index_start..*index_end, 0, 0..1);
                    }
                }
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }
}
