use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use renderer_core::{Color, DrawCommand, RenderBackend, RendererError};
use wgpu::{Device, Queue, Surface, SurfaceConfiguration, TextureViewDescriptor};

use crate::primitives::Viewport;
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
    RectBatch { start: u32, end: u32 },
    TextBatch { start: u32, end: u32 },
}

pub struct HardwareRenderer<W: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static> {
    surface: Surface<'static>,
    device: Device,
    queue: Queue,
    config: Option<SurfaceConfiguration>,
    rect_pipeline: RectPipeline,
    text_pipeline: TextPipeline,
    text_shaper: renderer_text::TextShaper,
    surface_format: wgpu::TextureFormat,
    present_mode: wgpu::PresentMode,
    alpha_mode: wgpu::CompositeAlphaMode,
    width: u32,
    height: u32,
    pending_instances: Vec<RectInstance>,
    pending_text_instances: Vec<TextInstance>,
    pending_steps: Vec<DrawStep>,
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
        .map_err(|_| RendererError::NoAdapter)?;

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("rsx-hardware-renderer"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            ..Default::default()
        }))
        .map_err(|e| RendererError::Device(e.to_string()))?;

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

        Ok(Self {
            surface,
            device,
            queue,
            config: None,
            rect_pipeline,
            text_pipeline,
            text_shaper: renderer_text::TextShaper::new(),
            surface_format,
            present_mode,
            alpha_mode,
            width: 0,
            height: 0,
            pending_instances: Vec::new(),
            pending_text_instances: Vec::new(),
            pending_steps: Vec::new(),
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
        self.pending_steps.clear();
        Ok(())
    }

    fn submit(&mut self, commands: &[DrawCommand]) {
        let mut current_rect_start: Option<u32> = None;
        let mut current_text_start: Option<u32> = None;

        for cmd in commands {
            match cmd {
                DrawCommand::Rect {
                    rect,
                    fill,
                    stroke,
                    radius,
                } => {
                    if let Some(start) = current_text_start.take() {
                        let end = self.pending_text_instances.len() as u32;
                        if end > start {
                            self.pending_steps.push(DrawStep::TextBatch { start, end });
                        }
                    }
                    if current_rect_start.is_none() {
                        current_rect_start = Some(self.pending_instances.len() as u32);
                    }
                    let inst = crate::primitives::rect::make_rect_instance(
                        *rect,
                        fill.as_ref(),
                        *stroke,
                        *radius,
                    );
                    self.pending_instances.push(inst);
                }
                DrawCommand::Text { text, rect, style } => {
                    if let Some(start) = current_rect_start.take() {
                        let end = self.pending_instances.len() as u32;
                        if end > start {
                            self.pending_steps.push(DrawStep::RectBatch { start, end });
                        }
                    }
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
                _ => {}
            }
        }

        if let Some(start) = current_rect_start {
            let end = self.pending_instances.len() as u32;
            if end > start {
                self.pending_steps.push(DrawStep::RectBatch { start, end });
            }
        }
        if let Some(start) = current_text_start {
            let end = self.pending_text_instances.len() as u32;
            if end > start {
                self.pending_steps.push(DrawStep::TextBatch { start, end });
            }
        }
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
            self.pending_steps.clear();
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
                self.pending_steps.clear();
                return Ok(());
            }
            other => {
                self.pending_instances.clear();
                self.pending_text_instances.clear();
                self.pending_steps.clear();
                return Err(RendererError::Present(format!("surface error: {other:?}")));
            }
        };

        let viewport = Viewport {
            size: [self.width as f32, self.height as f32],
            _pad: [0.0; 2],
        };
        self.queue.write_buffer(
            &self.rect_pipeline.viewport_buffer,
            0,
            bytemuck::bytes_of(&viewport),
        );
        self.queue.write_buffer(
            &self.text_pipeline.viewport_buffer,
            0,
            bytemuck::bytes_of(&viewport),
        );

        self.text_pipeline
            .sync_atlas(&self.queue, &mut self.text_shaper.atlas);

        let all_instances = std::mem::take(&mut self.pending_instances);
        let text_instances = std::mem::take(&mut self.pending_text_instances);
        let steps = std::mem::take(&mut self.pending_steps);

        if !all_instances.is_empty() {
            self.rect_pipeline
                .ensure_capacity(&self.device, all_instances.len());
            self.queue.write_buffer(
                &self.rect_pipeline.instances_buffer,
                0,
                bytemuck::cast_slice(&all_instances),
            );
        }

        if !text_instances.is_empty() {
            self.text_pipeline
                .ensure_capacity(&self.device, text_instances.len());
            self.queue.write_buffer(
                &self.text_pipeline.instances_buffer,
                0,
                bytemuck::cast_slice(&text_instances),
            );
        }

        let view = output
            .texture
            .create_view(&TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("rsx-encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rsx-render-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
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
                        render_pass.set_bind_group(0, &self.rect_pipeline.bind_group, &[]);
                        render_pass.draw(0..6, *start..*end);
                    }
                    DrawStep::TextBatch { start, end } => {
                        render_pass.set_pipeline(&self.text_pipeline.pipeline);
                        render_pass.set_bind_group(
                            0,
                            &self.text_pipeline.instances_bind_group,
                            &[],
                        );
                        render_pass.set_bind_group(1, &self.text_pipeline.atlas_bind_group, &[]);
                        render_pass.draw(0..6, *start..*end);
                    }
                }
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }
}
