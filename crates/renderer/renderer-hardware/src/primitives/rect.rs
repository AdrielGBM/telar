use renderer_core::{Rect, RectStyle};
use wgpu::Device;

use super::{InstancePipeline, MSAA_SAMPLES};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct RectInstance {
    pub rect: [f32; 4],
    pub radii: [f32; 4],
    // fill
    pub fill_type: u32,
    pub _pad_ft: [u32; 3],
    pub fill_color: [f32; 4],
    pub grad_p0: [f32; 2],
    pub grad_p1: [f32; 2],
    pub grad_radius: f32,
    pub grad_stop_count: u32,
    pub _pad_g: [f32; 2],
    pub grad_positions: [f32; 4],
    pub grad_colors: [[f32; 4]; 4],
    // stroke
    pub stroke_color: [f32; 4],
    pub stroke_width: f32,
    pub _pad: [f32; 3],
    // shadow (offset 208)
    pub shadow_color: [f32; 4],
    pub shadow_offset: [f32; 2],
    pub shadow_blur: f32,
    pub shadow_spread: f32,
}

// Typical frame has 100–200 rects; doubles on overflow, so reallocations are rare after warmup.
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
                count: MSAA_SAMPLES,
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

pub(crate) fn prepare_rect(rect: Rect, style: &RectStyle, tx: f32, ty: f32) -> RectInstance {
    let (
        fill_type,
        fill_color,
        grad_p0,
        grad_p1,
        grad_radius,
        grad_stop_count,
        grad_positions,
        grad_colors,
    ) = match style.fill {
        Some(renderer_core::FillStyle::Solid(c)) => (
            0u32,
            c.to_array(),
            [0.0f32; 2],
            [0.0f32; 2],
            0.0f32,
            0u32,
            [0.0f32; 4],
            [[0.0f32; 4]; 4],
        ),
        Some(renderer_core::FillStyle::LinearGradient(g)) => {
            let mut positions = [0.0f32; 4];
            let mut colors = [[0.0f32; 4]; 4];
            for i in 0..g.stop_count as usize {
                positions[i] = g.stops[i].position;
                colors[i] = g.stops[i].color.to_array();
            }
            (
                1u32,
                [0.0; 4],
                [g.start.x + tx, g.start.y + ty],
                [g.end.x + tx, g.end.y + ty],
                0.0,
                g.stop_count as u32,
                positions,
                colors,
            )
        }
        Some(renderer_core::FillStyle::RadialGradient(g)) => {
            let mut positions = [0.0f32; 4];
            let mut colors = [[0.0f32; 4]; 4];
            for i in 0..g.stop_count as usize {
                positions[i] = g.stops[i].position;
                colors[i] = g.stops[i].color.to_array();
            }
            (
                2u32,
                [0.0; 4],
                [g.center.x + tx, g.center.y + ty],
                [0.0, 0.0],
                g.radius,
                g.stop_count as u32,
                positions,
                colors,
            )
        }
        None => (
            0u32,
            [0.0; 4],
            [0.0; 2],
            [0.0; 2],
            0.0,
            0u32,
            [0.0; 4],
            [[0.0; 4]; 4],
        ),
    };

    let (stroke_color, stroke_width) = match style.stroke {
        Some(s) => (s.color.to_array(), s.width),
        None => ([0.0; 4], 0.0),
    };

    let (shadow_color, shadow_offset, shadow_blur, shadow_spread) = match style.shadow {
        Some(s) => (
            s.color.to_array(),
            [s.offset_x, s.offset_y],
            s.blur_radius,
            s.spread,
        ),
        None => ([0.0f32; 4], [0.0f32; 2], 0.0f32, 0.0f32),
    };

    RectInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        radii: [
            style.radius.top_left,
            style.radius.top_right,
            style.radius.bottom_right,
            style.radius.bottom_left,
        ],
        fill_type,
        _pad_ft: [0; 3],
        fill_color,
        grad_p0,
        grad_p1,
        grad_radius,
        grad_stop_count,
        _pad_g: [0.0; 2],
        grad_positions,
        grad_colors,
        stroke_color,
        stroke_width,
        _pad: [0.0; 3],
        shadow_color,
        shadow_offset,
        shadow_blur,
        shadow_spread,
    }
}
