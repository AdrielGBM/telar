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
            &[viewport_bgl, &instances.instances_bgl],
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
pub(crate) fn prepare_line(p1: Point, p2: Point, style: Stroke) -> LineInstance {
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
