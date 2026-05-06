use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use renderer_core::{BorderRadius, Color, Rect, RenderBackend, Stroke};
use wgpu::{Device, Queue, Surface, SurfaceConfiguration, TextureViewDescriptor};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Viewport {
    pub size: [f32; 2],
    pub _pad: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct RectInstance {
    pub rect: [f32; 4],
    pub radii: [f32; 4],
    pub fill_color: [f32; 4],
    pub stroke_color: [f32; 4],
    pub stroke_width: f32,
    pub _pad: [f32; 3],
}

const INITIAL_RECT_CAPACITY: usize = 256;

struct RectPipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    viewport_buffer: wgpu::Buffer,
    instances_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    instances_capacity: usize,
}

impl RectPipeline {
    fn new(device: &Device, surface_format: wgpu::TextureFormat) -> Self {
        let viewport_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rsx-rect-viewport"),
            size: std::mem::size_of::<Viewport>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let instances_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rsx-rect-instances"),
            size: (std::mem::size_of::<RectInstance>() * INITIAL_RECT_CAPACITY) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rsx-rect-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let bind_group = Self::make_bind_group(
            device,
            &bind_group_layout,
            &viewport_buffer,
            &instances_buffer,
        );

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rsx-rect-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("primitives/rect.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rsx-rect-pipeline-layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rsx-rect-pipeline"),
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
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            pipeline,
            bind_group_layout,
            viewport_buffer,
            instances_buffer,
            bind_group,
            instances_capacity: INITIAL_RECT_CAPACITY,
        }
    }

    fn make_bind_group(
        device: &Device,
        layout: &wgpu::BindGroupLayout,
        viewport_buffer: &wgpu::Buffer,
        instances_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rsx-rect-bg"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: viewport_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: instances_buffer.as_entire_binding(),
                },
            ],
        })
    }

    fn ensure_capacity(&mut self, device: &Device, count: usize) {
        if count <= self.instances_capacity {
            return;
        }
        let new_capacity = (count * 2).max(self.instances_capacity * 2);
        self.instances_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rsx-rect-instances"),
            size: (std::mem::size_of::<RectInstance>() * new_capacity) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.bind_group = Self::make_bind_group(
            device,
            &self.bind_group_layout,
            &self.viewport_buffer,
            &self.instances_buffer,
        );
        self.instances_capacity = new_capacity;
    }
}

fn preferred_format(caps: &wgpu::SurfaceCapabilities) -> wgpu::TextureFormat {
    caps.formats
        .iter()
        .find(|f| f.is_srgb())
        .copied()
        .unwrap_or(caps.formats[0])
}

pub struct HardwareRenderer<W: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static> {
    surface: Surface<'static>,
    device: Device,
    queue: Queue,
    config: Option<SurfaceConfiguration>,
    rect_pipeline: RectPipeline,
    pub(crate) rect_queue: Vec<RectInstance>,
    clear_color: wgpu::Color,
    surface_format: wgpu::TextureFormat,
    present_mode: wgpu::PresentMode,
    alpha_mode: wgpu::CompositeAlphaMode,
    width: u32,
    height: u32,
    _window: std::sync::Arc<W>,
}

impl<W: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static> HardwareRenderer<W> {
    pub fn new(window: W) -> Result<Self, String> {
        let window = std::sync::Arc::new(window);

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
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

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = preferred_format(&surface_caps);
        let present_mode = surface_caps
            .present_modes
            .iter()
            .find(|&&m| m == wgpu::PresentMode::Mailbox)
            .copied()
            .unwrap_or(wgpu::PresentMode::Fifo);
        let alpha_mode = surface_caps.alpha_modes[0];

        let rect_pipeline = RectPipeline::new(&device, surface_format);

        Ok(Self {
            surface,
            device,
            queue,
            config: None,
            rect_pipeline,
            rect_queue: Vec::new(),
            clear_color: wgpu::Color::BLACK,
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

    fn clear(&mut self, color: Color) {
        self.clear_color = wgpu::Color {
            r: color.r as f64,
            g: color.g as f64,
            b: color.b as f64,
            a: color.a as f64,
        };
    }

    fn draw_rect(
        &mut self,
        rect: Rect,
        fill: Option<Color>,
        stroke: Option<Stroke>,
        radius: BorderRadius,
    ) {
        self.draw_rect_impl(rect, fill, stroke, radius);
    }

    fn end_frame(&mut self) {
        let instances = std::mem::take(&mut self.rect_queue);

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

        let viewport = Viewport {
            size: [self.width as f32, self.height as f32],
            _pad: [0.0; 2],
        };
        self.queue.write_buffer(
            &self.rect_pipeline.viewport_buffer,
            0,
            bytemuck::bytes_of(&viewport),
        );

        let instance_count = instances.len();
        if instance_count > 0 {
            self.rect_pipeline
                .ensure_capacity(&self.device, instance_count);
            self.queue.write_buffer(
                &self.rect_pipeline.instances_buffer,
                0,
                bytemuck::cast_slice(&instances),
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
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            if instance_count > 0 {
                render_pass.set_pipeline(&self.rect_pipeline.pipeline);
                render_pass.set_bind_group(0, &self.rect_pipeline.bind_group, &[]);
                render_pass.draw(0..6, 0..instance_count as u32);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
    }
}
