use geometry_core::Point;
use renderer_core::{LineCap, Stroke};
use wgpu::Device;

use super::InstancePipeline;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct LineInstance {
    pub p1: [f32; 2],
    pub p2: [f32; 2],
    pub color: [f32; 4],
    pub width: f32,
    pub cap: f32,
    pub _pad: [f32; 2],
    // Paint encoding, as on `RectInstance`: `color` carries a solid stroke, these carry a gradient one.
    pub paint_type: u32,
    pub _pad_pt: [u32; 3],
    pub grad_p0: [f32; 2],
    pub grad_p1: [f32; 2],
    pub grad_radius: f32,
    pub grad_stop_count: u32,
    pub _pad_g: [f32; 2],
    pub grad_positions: [f32; 4],
    pub grad_colors: [[f32; 4]; 4],
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
        msaa_samples: u32,
    ) -> Self {
        let instances = InstancePipeline::<LineInstance>::new(device, "line", 256);

        let shader_source = [include_str!("viewport.wgsl"), include_str!("line.wgsl")].concat();
        let pipeline = super::create_render_pipeline(
            device,
            "line",
            &shader_source,
            &[viewport_bgl, &instances.instances_bind_group_layout],
            &[],
            surface_format,
            msaa_samples,
            cache,
        );

        Self {
            instances,
            pipeline,
        }
    }
}

#[inline]
/// `p1`/`p2` arrive already transformed into world space; `matrix` is that same transform, needed here to
/// place a gradient's endpoints in the space the fragment shader samples in.
pub(crate) fn prepare_line(p1: Point, p2: Point, style: Stroke, matrix: [f32; 6]) -> LineInstance {
    let cap = match style.cap {
        LineCap::Butt => 0.0f32,
        LineCap::Round => 1.0f32,
        LineCap::Square => 2.0f32,
    };
    let paint = super::encode_fill_style::<4>(&style.paint, matrix);
    LineInstance {
        p1: [p1.x, p1.y],
        p2: [p2.x, p2.y],
        color: paint.fill_color,
        width: style.width,
        cap,
        _pad: [0.0; 2],
        paint_type: paint.fill_type,
        _pad_pt: [0; 3],
        grad_p0: paint.grad_p0,
        grad_p1: paint.grad_p1,
        grad_radius: paint.grad_radius,
        grad_stop_count: paint.grad_stop_count,
        _pad_g: [0.0; 2],
        grad_positions: paint.grad_positions,
        grad_colors: paint.grad_colors,
    }
}
