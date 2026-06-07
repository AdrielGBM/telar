use geometry_core::Rect;
use renderer_core::RectStyle;
use wgpu::Device;

use super::{InstancePipeline, encode_fill_style};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct RectInstance {
    pub rect: [f32; 4],
    pub radii: [f32; 4],
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
    pub stroke_color: [f32; 4],
    pub stroke_width: f32,
    pub _pad: [f32; 3],
    // Always-present shadow fields using analytical GPU/SDF fast-path, zeroed when no shadow; unlike text/path shadows which use CPU capture+blur+composite
    pub shadow_color: [f32; 4],
    pub shadow_offset: [f32; 2],
    pub shadow_blur: f32,
    pub shadow_spread: f32,
    // transform (offset 240)
    pub transform: [f32; 6],
    pub _pad_t: [f32; 2],
}

pub(crate) struct RectPipeline {
    pub(crate) instances: InstancePipeline<RectInstance>,
    pub(crate) pipeline: wgpu::RenderPipeline,
}

impl RectPipeline {
    pub(crate) fn new(
        device: &Device,
        surface_format: wgpu::TextureFormat,
        viewport_bgl: &wgpu::BindGroupLayout,
        cache: Option<&wgpu::PipelineCache>,
        msaa_samples: u32,
    ) -> Self {
        let instances = InstancePipeline::<RectInstance>::new(device, "rect", 256);

        let shader_source = [include_str!("viewport.wgsl"), include_str!("rect.wgsl")].concat();
        let pipeline = super::create_render_pipeline(
            device,
            "rect",
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
pub(crate) fn prepare_rect(rect: Rect, style: &RectStyle, matrix: [f32; 6]) -> RectInstance {
    let encoded = style
        .fill
        .as_ref()
        .map(|fill| encode_fill_style(fill, matrix))
        .unwrap_or(super::EncodedFill {
            fill_type: 0,
            fill_color: [0.0; 4],
            grad_p0: [0.0; 2],
            grad_p1: [0.0; 2],
            grad_radius: 0.0,
            grad_stop_count: 0,
            grad_positions: [0.0; 4],
            grad_colors: [[0.0; 4]; 4],
        });

    let (stroke_color, stroke_width) = match style.stroke {
        Some(s) => (s.paint.solid_color().to_array(), s.width),
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
        fill_type: encoded.fill_type,
        _pad_ft: [0; 3],
        fill_color: encoded.fill_color,
        grad_p0: encoded.grad_p0,
        grad_p1: encoded.grad_p1,
        grad_radius: encoded.grad_radius,
        grad_stop_count: encoded.grad_stop_count,
        _pad_g: [0.0; 2],
        grad_positions: encoded.grad_positions,
        grad_colors: encoded.grad_colors,
        stroke_color,
        stroke_width,
        _pad: [0.0; 3],
        shadow_color,
        shadow_offset,
        shadow_blur,
        shadow_spread,
        transform: matrix,
        _pad_t: [0.0; 2],
    }
}
