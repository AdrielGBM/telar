//! The image pipeline: sampled quads, batched by texture.

use wgpu::Device;

use super::InstancePipeline;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct ImageInstance {
    pub dest_rect: [f32; 4],
}

/// The per-surface half of drawing images: instance buffers and a pipeline that bakes in this surface's format and sample count. The uploaded textures it samples are shared — see [`crate::caches::SharedImages`].
pub(crate) struct ImagePipeline {
    pub(crate) instances: InstancePipeline<ImageInstance>,
    pub(crate) pipeline: wgpu::RenderPipeline,
}

impl ImagePipeline {
    pub(crate) fn new(
        device: &Device,
        surface_format: wgpu::TextureFormat,
        viewport_bgl: &wgpu::BindGroupLayout,
        cache: Option<&wgpu::PipelineCache>,
        msaa_samples: u32,
        texture_bgl: &wgpu::BindGroupLayout,
    ) -> Self {
        let instances = InstancePipeline::<ImageInstance>::new(device, "image", 16);

        let shader_source = [include_str!("viewport.wgsl"), include_str!("image.wgsl")].concat();
        let pipeline = super::create_render_pipeline(
            device,
            "image",
            &shader_source,
            &[
                viewport_bgl,
                &instances.instances_bind_group_layout,
                texture_bgl,
            ],
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
pub(crate) fn prepare_image(rect: geometry_core::Rect) -> ImageInstance {
    ImageInstance {
        dest_rect: [rect.x, rect.y, rect.width, rect.height],
    }
}
