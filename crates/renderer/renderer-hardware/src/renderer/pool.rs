//! The scratch-texture pool layers and blur passes borrow from, bucketed by size and format.

use crate::primitives::Viewport;

// Prefer Rgba8Unorm: shaders output sRGB-encoded values, so the GPU must not encode again on write. Bgra8Unorm is the fallback for drivers that do not expose it.
pub(super) fn preferred_format(capabilities: &wgpu::SurfaceCapabilities) -> wgpu::TextureFormat {
    capabilities
        .formats
        .iter()
        .find(|f| matches!(f, wgpu::TextureFormat::Rgba8Unorm))
        .or_else(|| {
            capabilities
                .formats
                .iter()
                .find(|f| matches!(f, wgpu::TextureFormat::Bgra8Unorm))
        })
        .copied()
        .unwrap_or(capabilities.formats[0])
}

pub(super) fn create_viewport_pool_slot(
    device: &wgpu::Device,
    viewport_bind_group_layout: &wgpu::BindGroupLayout,
) -> (wgpu::Buffer, wgpu::BindGroup) {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("telar-layer-vp-pool"),
        size: std::mem::size_of::<Viewport>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("telar-layer-vp-pool-bg"),
        layout: viewport_bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    });
    (buffer, bind_group)
}

// The returned tuple must be handed back via `return_pooled_texture` once the frame's GPU work is recorded.
pub(super) fn take_pooled_texture(
    device: &wgpu::Device,
    pool: &mut Vec<(
        u32,
        u32,
        wgpu::TextureFormat,
        wgpu::Texture,
        wgpu::TextureView,
    )>,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    label: &str,
    usage: wgpu::TextureUsages,
) -> (
    u32,
    u32,
    wgpu::TextureFormat,
    wgpu::Texture,
    wgpu::TextureView,
) {
    if let Some(pos) = pool
        .iter()
        .position(|(w, h, f, _, _)| *w == width && *h == height && *f == format)
    {
        return pool.swap_remove(pos);
    }
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (width, height, format, texture, view)
}

// Bounded to `max_per_size` entries per bucket, so memory does not grow without limit.
pub(super) fn return_pooled_texture(
    pool: &mut Vec<(
        u32,
        u32,
        wgpu::TextureFormat,
        wgpu::Texture,
        wgpu::TextureView,
    )>,
    entry: (
        u32,
        u32,
        wgpu::TextureFormat,
        wgpu::Texture,
        wgpu::TextureView,
    ),
    max_per_size: usize,
) {
    let (w, h, f, _, _) = entry;
    let count = pool
        .iter()
        .filter(|(pw, ph, pf, _, _)| *pw == w && *ph == h && *pf == f)
        .count();
    if count < max_per_size {
        pool.push(entry);
    }
}

// Rounded up to a multiple of 64, so pool textures are reused across subpixel-layout variations that produce slightly different exact dimensions.
pub(super) fn bucket_size(size: u32) -> u32 {
    const B: u32 = 64;
    size.div_ceil(B) * B
}

pub(super) struct PooledTexture {
    pub(super) msaa_texture: wgpu::Texture,
    pub(super) msaa_view: wgpu::TextureView,
    pub(super) resolve_texture: wgpu::Texture,
    pub(super) resolve_view: wgpu::TextureView,
    // Physical pixel dimensions of the bucket this texture was allocated for.
    pub(super) bucket_width: u32,
    pub(super) bucket_height: u32,
}

/// The MSAA and resolve pair for a layer at `bucket_w`×`bucket_h`: reused from `pool` when it holds a matching bucket, freshly created when it does not.
///
/// Takes the pool by reference rather than reading it off the renderer, so the layer pool and the shadow capture pool — which asked the same question in the same twenty-two lines — can both call it.
pub(super) fn take_layer_textures(
    pool: &mut Vec<PooledTexture>,
    pipeline: &crate::primitives::layer::LayerPipeline,
    device: &wgpu::Device,
    bucket_w: u32,
    bucket_h: u32,
) -> (
    wgpu::Texture,
    wgpu::TextureView,
    wgpu::Texture,
    wgpu::TextureView,
) {
    match pool
        .iter()
        .position(|p| p.bucket_width == bucket_w && p.bucket_height == bucket_h)
    {
        Some(pos) => {
            let p = pool.remove(pos);
            (
                p.msaa_texture,
                p.msaa_view,
                p.resolve_texture,
                p.resolve_view,
            )
        }
        None => pipeline.create_layer_textures(device, bucket_w, bucket_h),
    }
}
