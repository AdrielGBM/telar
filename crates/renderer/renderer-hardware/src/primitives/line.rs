use geometry_core::Point;
use renderer_core::{LineCap, LineStyle};
use wgpu::Device;

use super::{InstancePipeline, MSAA_SAMPLES};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct LineInstance {
    pub p1: [f32; 2],
    pub p2: [f32; 2],
    pub color: [f32; 4],
    pub width: f32,
    pub cap: f32,
    pub _pad: [f32; 2],
}

pub(crate) struct LinePipeline {
    pub(crate) instances: InstancePipeline<LineInstance>,
    pub(crate) pipeline: wgpu::RenderPipeline,
}

impl LinePipeline {
    pub(crate) fn new(
        device: &Device,
        surface_format: wgpu::TextureFormat,
        viewport_bgl: &wgpu::BindGroupLayout,
        cache: Option<&wgpu::PipelineCache>,
    ) -> Self {
        let instances = InstancePipeline::<LineInstance>::new(device, "line", 256);

        let shader_source = [include_str!("viewport.wgsl"), include_str!("line.wgsl")].concat();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rsx-line-shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rsx-line-pipeline-layout"),
            bind_group_layouts: &[Some(viewport_bgl), Some(&instances.instances_bgl)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rsx-line-pipeline"),
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
            multisample: wgpu::MultisampleState {
                count: MSAA_SAMPLES,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache,
        });

        Self {
            instances,
            pipeline,
        }
    }
}

#[inline]
pub(crate) fn prepare_line(p1: Point, p2: Point, style: LineStyle) -> LineInstance {
    let cap = match style.cap {
        LineCap::Butt => 0.0f32,
        LineCap::Round => 1.0f32,
        LineCap::Square => 2.0f32,
    };
    LineInstance {
        p1: [p1.x, p1.y],
        p2: [p2.x, p2.y],
        color: style.paint.solid_color().to_array(),
        width: style.width,
        cap,
        _pad: [0.0; 2],
    }
}
