use renderer_core::{Rect, RectStyle};
use wgpu::Device;

use super::InstancePipeline;

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

pub(crate) const INITIAL_RECT_CAPACITY: usize = 256;

pub(crate) struct RectPipeline {
    pub(crate) instances: InstancePipeline<RectInstance>,
    pub(crate) pipeline: wgpu::RenderPipeline,
}

impl RectPipeline {
    pub(crate) fn new(
        device: &Device,
        surface_format: wgpu::TextureFormat,
        viewport_bgl: &wgpu::BindGroupLayout,
    ) -> Self {
        let instances =
            InstancePipeline::<RectInstance>::new(device, "rect", INITIAL_RECT_CAPACITY);

        let shader_source = [include_str!("viewport.wgsl"), include_str!("rect.wgsl")].concat();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rsx-rect-shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rsx-rect-pipeline-layout"),
            bind_group_layouts: &[Some(viewport_bgl), Some(&instances.instances_bgl)],
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
            multisample: wgpu::MultisampleState {
                count: 4,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });

        Self {
            instances,
            pipeline,
        }
    }
}

pub(crate) fn prepare_rect(rect: Rect, style: &RectStyle) -> RectInstance {
    let fill_color = match style.fill {
        Some(renderer_core::FillStyle::Solid(c)) => [c.r, c.g, c.b, c.a],
        None => [0.0; 4],
    };
    let (stroke_color, stroke_width) = match style.stroke {
        Some(s) => ([s.color.r, s.color.g, s.color.b, s.color.a], s.width),
        None => ([0.0; 4], 0.0),
    };
    RectInstance {
        rect: [rect.x, rect.y, rect.w, rect.h],
        radii: [
            style.radius.top_left,
            style.radius.top_right,
            style.radius.bottom_right,
            style.radius.bottom_left,
        ],
        fill_color,
        stroke_color,
        stroke_width,
        _pad: [0.0; 3],
    }
}
