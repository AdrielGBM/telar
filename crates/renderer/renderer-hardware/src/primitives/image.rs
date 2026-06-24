use rustc_hash::FxHashMap;
use std::sync::Arc;

use renderer_core::{ImageData, ImageFilter};
use wgpu::Device;

use super::InstancePipeline;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct ImageInstance {
    pub dest_rect: [f32; 4],
}

struct GpuImage {
    // Keeps the GPU texture alive via RAII; held to ensure the texture is not dropped while the image is in use.
    #[allow(dead_code)]
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    last_used_frame: u64,
}

pub(crate) struct ImagePipeline {
    pub(crate) instances: InstancePipeline<ImageInstance>,
    pub(crate) pipeline: wgpu::RenderPipeline,
    texture_bgl: wgpu::BindGroupLayout,
    sampler_nearest: wgpu::Sampler,
    sampler_linear: wgpu::Sampler,
    texture_cache: FxHashMap<(u64, ImageFilter), GpuImage>,
    current_frame: u64,
    gpu_max_age_frames: u64,
}

impl ImagePipeline {
    pub(crate) fn new(
        device: &Device,
        surface_format: wgpu::TextureFormat,
        viewport_bgl: &wgpu::BindGroupLayout,
        cache: Option<&wgpu::PipelineCache>,
        msaa_samples: u32,
        gpu_max_age_frames: u64,
    ) -> Self {
        let instances = InstancePipeline::<ImageInstance>::new(device, "image", 16);

        let texture_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rsx-image-texture-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let sampler_nearest = create_sampler(device, wgpu::FilterMode::Nearest);
        let sampler_linear = create_sampler(device, wgpu::FilterMode::Linear);

        let shader_source = [include_str!("viewport.wgsl"), include_str!("image.wgsl")].concat();
        let pipeline = super::create_render_pipeline(
            device,
            "image",
            &shader_source,
            &[viewport_bgl, &instances.instances_bgl, &texture_bgl],
            &[],
            surface_format,
            msaa_samples,
            cache,
        );

        Self {
            instances,
            pipeline,
            texture_bgl,
            sampler_nearest,
            sampler_linear,
            texture_cache: FxHashMap::default(),
            current_frame: 0,
            gpu_max_age_frames,
        }
    }

    fn create_gpu_image(
        &self,
        device: &Device,
        queue: &wgpu::Queue,
        image: &Arc<ImageData>,
        filter: ImageFilter,
    ) -> GpuImage {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rsx-image-texture"),
            size: wgpu::Extent3d {
                width: image.width,
                height: image.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &image.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * image.width),
                rows_per_image: Some(image.height),
            },
            wgpu::Extent3d {
                width: image.width,
                height: image.height,
                depth_or_array_layers: 1,
            },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = match filter {
            ImageFilter::Nearest => &self.sampler_nearest,
            ImageFilter::Linear => &self.sampler_linear,
        };

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rsx-image-texture-bg"),
            layout: &self.texture_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });

        GpuImage {
            texture,
            bind_group,
            last_used_frame: self.current_frame,
        }
    }

    pub(crate) fn get_or_create_bind_group(
        &mut self,
        device: &Device,
        queue: &wgpu::Queue,
        image: &Arc<ImageData>,
        filter: ImageFilter,
    ) -> wgpu::BindGroup {
        let key = (image.id, filter);
        if !self.texture_cache.contains_key(&key) {
            let gpu_image = self.create_gpu_image(device, queue, image, filter);
            self.texture_cache.insert(key, gpu_image);
        }
        let entry = self.texture_cache.get_mut(&key).unwrap();
        entry.last_used_frame = self.current_frame;
        entry.bind_group.clone()
    }

    fn evict_unused(&mut self, max_age_frames: u64) {
        let current = self.current_frame;
        self.texture_cache
            .retain(|_, gpu_image| current - gpu_image.last_used_frame <= max_age_frames);
    }

    pub fn begin_frame(&mut self) {
        self.current_frame += 1;
        self.evict_unused(self.gpu_max_age_frames);
    }
}

fn create_sampler(device: &Device, filter: wgpu::FilterMode) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: None,
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: filter,
        min_filter: filter,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    })
}

#[inline]
pub(crate) fn prepare_image(rect: geometry_core::Rect) -> ImageInstance {
    ImageInstance {
        dest_rect: [rect.x, rect.y, rect.width, rect.height],
    }
}
