use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use renderer_core::{Color, RenderBackend};
use wgpu::{
    Adapter, Device, Instance, Queue, RenderPipeline, Surface, SurfaceConfiguration,
    TextureViewDescriptor,
};

fn preferred_format(caps: &wgpu::SurfaceCapabilities) -> wgpu::TextureFormat {
    caps.formats
        .iter()
        .find(|f| f.is_srgb())
        .copied()
        .unwrap_or(caps.formats[0])
}

pub struct HardwareRenderer<W: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static> {
    surface: Surface<'static>,
    adapter: Adapter,
    device: Device,
    queue: Queue,
    config: Option<SurfaceConfiguration>,
    pipeline: RenderPipeline,
    clear_color: wgpu::Color,
    width: u32,
    height: u32,
    _window: std::sync::Arc<W>,
}

impl<W: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static> HardwareRenderer<W> {
    pub fn new(window: W) -> Result<Self, String> {
        let window = std::sync::Arc::new(window);

        let instance = Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| e.to_string())?;

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .ok_or_else(|| "No suitable GPU adapter found".to_string())?;

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("rsx-hardware-renderer"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::default(),
            },
            None,
        ))
        .map_err(|e| e.to_string())?;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rsx-passthrough"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rsx-pipeline-layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = preferred_format(&surface_caps);

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rsx-render-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        Ok(Self {
            surface,
            adapter,
            device,
            queue,
            config: None,
            pipeline,
            clear_color: wgpu::Color::BLACK,
            width: 0,
            height: 0,
            _window: window,
        })
    }

    fn reconfigure(&mut self, width: u32, height: u32) {
        let surface_caps = self.surface.get_capabilities(&self.adapter);
        let format = preferred_format(&surface_caps);

        let config = SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: surface_caps
                .present_modes
                .iter()
                .find(|&&m| m == wgpu::PresentMode::Mailbox)
                .copied()
                .unwrap_or(wgpu::PresentMode::Fifo),
            alpha_mode: surface_caps.alpha_modes[0],
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

    fn clear(&mut self, color: Color) {
        self.clear_color = wgpu::Color {
            r: color.r as f64,
            g: color.g as f64,
            b: color.b as f64,
            a: color.a as f64,
        };
    }

    fn end_frame(&mut self) {
        if self.config.is_none() || self.width == 0 || self.height == 0 {
            return;
        }

        let output = match self.surface.get_current_texture() {
            Ok(texture) => texture,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                if let Some(config) = &self.config.clone() {
                    self.surface.configure(&self.device, config);
                }
                return;
            }
            Err(_) => return,
        };

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
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            render_pass.set_pipeline(&self.pipeline);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
    }
}
