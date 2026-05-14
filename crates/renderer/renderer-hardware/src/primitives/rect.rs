use renderer_core::{BorderRadius, Rect, Stroke};
use wgpu::Device;

use super::Viewport;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct RectInstance {
    pub rect: [f32; 4],
    pub radii: [f32; 4],
    pub fill_color: [f32; 4],
    pub stroke_color: [f32; 4],
    pub stroke_width: f32,
    pub _pad0: f32,
    pub _pad1: f32,
    pub _pad2: f32,
}

pub(crate) const INITIAL_RECT_CAPACITY: usize = 256;

pub(crate) struct RectPipeline {
    pub(crate) pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    pub(crate) viewport_buffer: wgpu::Buffer,
    pub(crate) instances_buffer: wgpu::Buffer,
    pub(crate) bind_group: wgpu::BindGroup,
    instances_capacity: usize,
}

impl RectPipeline {
    pub(crate) fn new(device: &Device, surface_format: wgpu::TextureFormat) -> Self {
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
            source: wgpu::ShaderSource::Wgsl(include_str!("rect.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rsx-rect-pipeline-layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
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
            multiview_mask: None,
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

    pub(crate) fn ensure_capacity(&mut self, device: &Device, count: usize) {
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

pub(crate) fn make_rect_instance(
    rect: Rect,
    fill: Option<renderer_core::FillStyle>,
    stroke: Option<Stroke>,
    radius: BorderRadius,
) -> RectInstance {
    let fill_color = match fill {
        Some(renderer_core::FillStyle::Solid(c)) => [c.r, c.g, c.b, c.a],
        None => [0.0; 4],
    };
    let (stroke_color, stroke_width) = match stroke {
        Some(s) => ([s.color.r, s.color.g, s.color.b, s.color.a], s.width),
        None => ([0.0; 4], 0.0),
    };
    RectInstance {
        rect: [rect.x, rect.y, rect.w, rect.h],
        radii: [
            radius.top_left,
            radius.top_right,
            radius.bottom_right,
            radius.bottom_left,
        ],
        fill_color,
        stroke_color,
        stroke_width,
        _pad0: 0.0,
        _pad1: 0.0,
        _pad2: 0.0,
    }
}
