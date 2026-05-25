use super::MSAA_SAMPLES;

pub(crate) struct LayerPipeline {
    target_format: wgpu::TextureFormat,
}

impl LayerPipeline {
    pub(crate) fn new(_device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        Self { target_format }
    }

    pub(crate) fn create_layer_textures(
        &self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> (
        wgpu::Texture,
        wgpu::TextureView,
        wgpu::Texture,
        wgpu::TextureView,
    ) {
        let msaa = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rsx-layer-msaa"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: MSAA_SAMPLES,
            dimension: wgpu::TextureDimension::D2,
            format: self.target_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let msaa_view = msaa.create_view(&wgpu::TextureViewDescriptor::default());
        let resolve = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rsx-layer-resolve"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.target_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let resolve_view = resolve.create_view(&wgpu::TextureViewDescriptor::default());
        (msaa, msaa_view, resolve, resolve_view)
    }
}
