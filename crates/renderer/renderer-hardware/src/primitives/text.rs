//! The text pipeline: glyph quads sampled from the shared atlas.

use geometry_core::Rect;
use renderer_core::TextStyle;
use wgpu::Device;

use super::InstancePipeline;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct TextInstance {
    pub dest_rect: [f32; 4],
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
    pub color: [f32; 4],
}

/// The per-surface half of drawing text: the instance buffers a frame fills, and a pipeline that bakes in this surface's format and sample count. The atlas it samples is shared — see [`crate::caches::SharedAtlas`].
pub(crate) struct TextPipeline {
    pub(crate) instances: InstancePipeline<TextInstance>,
    pub(crate) pipeline: wgpu::RenderPipeline,
    /// A handle on the shared atlas's bind group, not a second one: `wgpu`'s resources are `Arc`s, so this names the same GPU object every other renderer draws from.
    pub(crate) atlas_bind_group: wgpu::BindGroup,
}

impl TextPipeline {
    pub(crate) fn new(
        device: &Device,
        surface_format: wgpu::TextureFormat,
        viewport_bgl: &wgpu::BindGroupLayout,
        cache: Option<&wgpu::PipelineCache>,
        msaa_samples: u32,
        atlas_bgl: &wgpu::BindGroupLayout,
        atlas_bind_group: wgpu::BindGroup,
    ) -> Self {
        let instances = InstancePipeline::<TextInstance>::new(device, "text", 256);

        let shader_source = [include_str!("viewport.wgsl"), include_str!("text.wgsl")].concat();
        let pipeline = super::create_render_pipeline(
            device,
            "text",
            &shader_source,
            &[
                viewport_bgl,
                &instances.instances_bind_group_layout,
                atlas_bgl,
            ],
            &[],
            surface_format,
            msaa_samples,
            cache,
        );

        Self {
            instances,
            pipeline,
            atlas_bind_group,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_text(
    shaper: &mut renderer_text::TextShaper,
    text: &str,
    spans: Option<&[renderer_core::Span]>,
    rect: Rect,
    style: &TextStyle,
    scale_factor: f32,
    out: &mut Vec<TextInstance>,
    glyph_scratch: &mut Vec<renderer_text::GlyphInfo>,
) {
    glyph_scratch.clear();
    shaper.layout_glyphs(text, spans, rect, style, scale_factor, glyph_scratch);
    out.extend(glyph_scratch.iter().map(|g| TextInstance {
        dest_rect: g.dest_rect,
        uv_min: g.uv_min,
        uv_max: g.uv_max,
        color: g.color,
    }));
}
