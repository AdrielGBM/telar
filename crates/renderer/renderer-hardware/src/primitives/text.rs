use geometry_core::Rect;
use renderer_core::TextStyle;
use renderer_text::{ATLAS_SIZE, GlyphAtlas};
use wgpu::{Device, Queue};

use super::InstancePipeline;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct TextInstance {
    pub dest_rect: [f32; 4],
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
    pub color: [f32; 4],
}

pub(crate) struct TextPipeline {
    pub(crate) instances: InstancePipeline<TextInstance>,
    pub(crate) pipeline: wgpu::RenderPipeline,
    atlas_texture: wgpu::Texture,
    pub(crate) atlas_bind_group: wgpu::BindGroup,
}

impl TextPipeline {
    pub(crate) fn new(
        device: &Device,
        surface_format: wgpu::TextureFormat,
        viewport_bgl: &wgpu::BindGroupLayout,
        cache: Option<&wgpu::PipelineCache>,
        msaa_samples: u32,
    ) -> Self {
        let instances = InstancePipeline::<TextInstance>::new(device, "text", 256);

        let atlas_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("telar-text-atlas-bgl"),
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

        let atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("telar-text-atlas"),
            size: wgpu::Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let atlas_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("telar-text-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let atlas_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("telar-text-atlas-bg"),
            layout: &atlas_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let shader_source = [include_str!("viewport.wgsl"), include_str!("text.wgsl")].concat();
        let pipeline = super::create_render_pipeline(
            device,
            "text",
            &shader_source,
            &[
                viewport_bgl,
                &instances.instances_bind_group_layout,
                &atlas_bgl,
            ],
            &[],
            surface_format,
            msaa_samples,
            cache,
        );

        Self {
            instances,
            pipeline,
            atlas_texture,
            atlas_bind_group,
        }
    }

    pub(crate) fn sync_atlas(&self, queue: &Queue, atlas: &mut GlyphAtlas) {
        // Collect the dirty rects into a local vec so the mutable borrow from `drain_dirty_rects()` is released before we read `atlas.pixels`.
        let dirty: Vec<[u32; 4]> = atlas.drain_dirty_rects().collect();
        for [x, y, w, h] in dirty {
            if w == 0 || h == 0 {
                continue;
            }
            // Upload only the dirty sub-rectangle. By providing the source slice starting at the (x, y) pixel and using the FULL atlas row stride for bytes_per_row, wgpu reads `w` pixels per row for `h` rows starting at (x, y) — the correct sub-rect upload pattern.
            let offset = ((y as usize * ATLAS_SIZE as usize) + x as usize) * 4;
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.atlas_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x, y, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                &atlas.pixels[offset..],
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * ATLAS_SIZE),
                    rows_per_image: None,
                },
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
}

pub(crate) fn prepare_text(
    shaper: &mut renderer_text::TextShaper,
    text: &str,
    rect: Rect,
    style: &TextStyle,
    scale_factor: f32,
    out: &mut Vec<TextInstance>,
    glyph_scratch: &mut Vec<renderer_text::GlyphInfo>,
) {
    glyph_scratch.clear();
    shaper.layout_glyphs(text, rect, style, scale_factor, glyph_scratch);
    out.extend(glyph_scratch.iter().map(|g| TextInstance {
        dest_rect: g.dest_rect,
        uv_min: g.uv_min,
        uv_max: g.uv_max,
        color: g.color,
    }));
}

pub(crate) fn prepare_rich_text(
    shaper: &mut renderer_text::TextShaper,
    runs: &[renderer_core::TextRun],
    rect: Rect,
    base: &TextStyle,
    scale_factor: f32,
    out: &mut Vec<TextInstance>,
    glyph_scratch: &mut Vec<renderer_text::GlyphInfo>,
) {
    glyph_scratch.clear();
    shaper.layout_glyphs_rich(runs, rect, base, scale_factor, glyph_scratch);
    out.extend(glyph_scratch.iter().map(|g| TextInstance {
        dest_rect: g.dest_rect,
        uv_min: g.uv_min,
        uv_max: g.uv_max,
        color: g.color,
    }));
}
