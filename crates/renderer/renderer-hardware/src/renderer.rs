use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use renderer_core::{
    BorderRadius, Color, DrawCommand, FillStyle, Rect, RenderBackend, RendererError, Stroke,
    TextCacheKey, TextStyle,
};
use std::collections::{HashMap, HashSet};
use wgpu::{Device, Queue, Surface, SurfaceConfiguration, TextureViewDescriptor};

use crate::primitives::Viewport;
use crate::primitives::rect::{RectInstance, RectPipeline};
use crate::primitives::text::{PreparedTextDraw, TextPipeline};

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

pub struct HardwareRenderer<W: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static> {
    surface: Surface<'static>,
    device: Device,
    queue: Queue,
    config: Option<SurfaceConfiguration>,
    rect_pipeline: RectPipeline,
    text_pipeline: TextPipeline,
    draw_commands: Vec<DrawCommand>,
    text_shaper: renderer_core::TextShaper,
    text_gpu_cache: HashMap<TextCacheKey, PreparedTextDraw>,
    surface_format: wgpu::TextureFormat,
    present_mode: wgpu::PresentMode,
    alpha_mode: wgpu::CompositeAlphaMode,
    width: u32,
    height: u32,
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
            draw_commands: Vec::new(),
            text_shaper: renderer_core::TextShaper::new(),
            text_gpu_cache: HashMap::new(),
            surface_format,
            present_mode,
            alpha_mode,
            width: 0,
            height: 0,
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
    fn begin_frame(&mut self, width: u32, height: u32) {
        if width != self.width || height != self.height || self.config.is_none() {
            self.width = width;
            self.height = height;
            if width > 0 && height > 0 {
                self.reconfigure(width, height);
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

        let commands = std::mem::take(&mut self.draw_commands);

        if self.config.is_none() || self.width == 0 || self.height == 0 {
            return;
        }

        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                if let Some(config) = &self.config.clone() {
                    self.surface.configure(&self.device, config);
                }
                return;
            }
            other => {
                eprintln!("rsx: surface error: {other:?}");
                return;
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

        enum DrawStep {
            RectBatch { start: u32, end: u32 },
            TextIdx(usize),
        }

        let mut all_instances: Vec<RectInstance> = Vec::new();
        let mut text_draws: Vec<crate::primitives::text::TextDraw> = Vec::new();
        let mut steps: Vec<DrawStep> = Vec::new();
        let mut current_batch_start: Option<u32> = None;

        for cmd in commands {
            match cmd {
                DrawCommand::Rect {
                    rect,
                    fill,
                    stroke,
                    radius,
                } => {
                    let inst = crate::primitives::rect::make_rect_instance(
                        rect,
                        fill.as_ref(),
                        stroke,
                        radius,
                    );
                    if current_batch_start.is_none() {
                        current_batch_start = Some(all_instances.len() as u32);
                    }
                    all_instances.push(inst);
                }
                DrawCommand::Text { text, rect, style } => {
                    if let Some(start) = current_batch_start.take() {
                        let end = all_instances.len() as u32;
                        if end > start {
                            steps.push(DrawStep::RectBatch { start, end });
                        }
                    }
                    let (cache_key, pixels, width, height) =
                        self.text_shaper
                            .rasterize(&text, rect, style.font_size, style.color);
                    if width > 0 && height > 0 {
                        let idx = text_draws.len();
                        text_draws.push(crate::primitives::text::TextDraw {
                            pixels,
                            rect,
                            width,
                            height,
                            cache_key,
                        });
                        steps.push(DrawStep::TextIdx(idx));
                    }
                }
            }
        }
        if let Some(start) = current_batch_start {
            let end = all_instances.len() as u32;
            if end > start {
                steps.push(DrawStep::RectBatch { start, end });
            }
        }

        if !all_instances.is_empty() {
            self.rect_pipeline
                .ensure_capacity(&self.device, all_instances.len());
            self.queue.write_buffer(
                &self.rect_pipeline.instances_buffer,
                0,
                bytemuck::cast_slice(&all_instances),
            );
        }

        let frame_keys: HashSet<_> = text_draws.iter().map(|td| td.cache_key.clone()).collect();

        self.text_gpu_cache.retain(|k, _| frame_keys.contains(k));

        for td in &text_draws {
            if !self.text_gpu_cache.contains_key(&td.cache_key) {
                let prepared = self
                    .text_pipeline
                    .prepare_draw(&self.device, &self.queue, td);
                self.text_gpu_cache.insert(td.cache_key.clone(), prepared);
            }
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
                    DrawStep::TextIdx(idx) => {
                        let td = &text_draws[*idx];
                        if let Some(prepared) = self.text_gpu_cache.get(&td.cache_key) {
                            render_pass.set_pipeline(&self.text_pipeline.pipeline);
                            render_pass.set_bind_group(0, &prepared.group0, &[]);
                            render_pass.set_bind_group(1, &prepared.group1, &[]);
                            render_pass.draw(0..6, 0..1);
                        }
                    }
                }
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
    }
}
