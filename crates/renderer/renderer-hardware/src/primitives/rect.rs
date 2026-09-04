//! The rect pipeline: rounded, bordered and shadowed boxes drawn by an SDF in one instanced pass.

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
    // `[top, right, bottom, left]`, filling exactly the slot the single width plus its three padding floats took.
    pub stroke_widths: [f32; 4],
    // Analytical SDF fast path, zeroed when there is no shadow — unlike text and path shadows, which capture, blur and composite on the CPU.
    pub shadow_color: [f32; 4],
    pub shadow_offset: [f32; 2],
    pub shadow_blur: f32,
    pub shadow_spread: f32,
    // transform (offset 240)
    pub transform: [f32; 6],
    pub _pad_t: [f32; 2],
    // An element can carry a gradient fill and a gradient stroke, so the stroke cannot borrow the fill's slots.
    pub stroke_type: u32,
    pub _pad_st: [u32; 3],
    pub stroke_grad_p0: [f32; 2],
    pub stroke_grad_p1: [f32; 2],
    pub stroke_grad_radius: f32,
    pub stroke_grad_stop_count: u32,
    pub _pad_sg: [f32; 2],
    pub stroke_grad_positions: [f32; 4],
    pub stroke_grad_colors: [[f32; 4]; 4],
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
pub(crate) fn prepare_rect(rect: Rect, style: &RectStyle, matrix: [f32; 6]) -> RectInstance {
    let encoded = style
        .fill
        .as_ref()
        .map(|fill| encode_fill_style::<4>(fill, matrix))
        .unwrap_or_else(super::EncodedFill::none);

    // Encoded like the fill, so a gradient stroke keeps its whole ramp instead of collapsing to its first stop.
    let (stroke, stroke_widths) = match style.painted_border() {
        Some((paint, widths)) => (encode_fill_style::<4>(&paint, matrix), widths),
        None => (super::EncodedFill::none(), [0.0; 4]),
    };
    let stroke_color = stroke.fill_color;

    let (shadow_color, shadow_offset, shadow_blur, shadow_spread) = match style.shadow {
        Some(s) => (
            s.color.to_array(),
            [s.offset_x, s.offset_y],
            s.blur_radius,
            s.spread,
        ),
        None => ([0.0f32; 4], [0.0f32; 2], 0.0f32, 0.0f32),
    };

    // The SDF degenerates past radius = min(w, h) / 2, so clamping there keeps an oversized radius identical on both backends.
    let max_r = (rect.width.min(rect.height) * 0.5).max(0.0);
    RectInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        radii: [
            style.radius.top_left.min(max_r),
            style.radius.top_right.min(max_r),
            style.radius.bottom_right.min(max_r),
            style.radius.bottom_left.min(max_r),
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
        stroke_widths,
        shadow_color,
        shadow_offset,
        shadow_blur,
        shadow_spread,
        transform: matrix,
        _pad_t: [0.0; 2],
        stroke_type: stroke.fill_type,
        _pad_st: [0; 3],
        stroke_grad_p0: stroke.grad_p0,
        stroke_grad_p1: stroke.grad_p1,
        stroke_grad_radius: stroke.grad_radius,
        stroke_grad_stop_count: stroke.grad_stop_count,
        _pad_sg: [0.0; 2],
        stroke_grad_positions: stroke.grad_positions,
        stroke_grad_colors: stroke.grad_colors,
    }
}
