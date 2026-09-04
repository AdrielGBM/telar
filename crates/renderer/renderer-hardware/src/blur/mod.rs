//! Backdrop blur, and the sigma at which it switches from a Gaussian to the dual-filter chain.

use wgpu::{Device, TextureFormat};

mod dual_filter;
mod gaussian;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct BlurParams {
    direction: [f32; 2],
    texture_size: [f32; 2],
    sigma: f32,
    _pad: [f32; 3],
}

pub(crate) struct BlurPipeline {
    pipeline: wgpu::RenderPipeline,
    // Used above `DUAL_FILTER_SIGMA_THRESHOLD` to cut GPU bandwidth versus the separable Gaussian.
    downsample_pipeline: wgpu::RenderPipeline,
    upsample_pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    bind_group_layout: wgpu::BindGroupLayout,
    format: TextureFormat,
    use_immediates: bool,
    // Keyed by (width, height). The intermediate is loaded with Clear every horizontal pass, so leftover contents are harmless.
    intermediate_pool: Vec<(wgpu::Texture, wgpu::TextureView, u32, u32)>,
}

// Above this sigma the separable Gaussian samples too many texels at full resolution.
const DUAL_FILTER_SIGMA_THRESHOLD: f32 = 3.0;

impl BlurPipeline {
    pub(crate) fn new(
        device: &Device,
        format: TextureFormat,
        cache: Option<&wgpu::PipelineCache>,
        use_immediates: bool,
    ) -> Self {
        let (shader_source, downsample_source, upsample_source) = if use_immediates {
            (
                include_str!("../blur_immediates.wgsl"),
                include_str!("../blur_downsample_immediates.wgsl"),
                include_str!("../blur_upsample_immediates.wgsl"),
            )
        } else {
            (
                include_str!("../blur.wgsl"),
                include_str!("../blur_downsample.wgsl"),
                include_str!("../blur_upsample.wgsl"),
            )
        };
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("telar-blur-shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });
        let downsample_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("telar-blur-downsample-shader"),
            source: wgpu::ShaderSource::Wgsl(downsample_source.into()),
        });
        let upsample_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("telar-blur-upsample-shader"),
            source: wgpu::ShaderSource::Wgsl(upsample_source.into()),
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("telar-blur-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let mut bind_group_layout_entries = vec![
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
        ];
        if !use_immediates {
            bind_group_layout_entries.push(wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(
                        std::mem::size_of::<BlurParams>() as u64
                    ),
                },
                count: None,
            });
        }
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("telar-blur-bgl"),
            entries: &bind_group_layout_entries,
        });

        let immediate_size = if use_immediates {
            std::mem::size_of::<BlurParams>() as u32
        } else {
            0
        };
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("telar-blur-pipeline-layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("telar-blur-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache,
        });

        let make_dual_pipeline =
            |label: &str, module: &wgpu::ShaderModule| -> wgpu::RenderPipeline {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(label),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module,
                        entry_point: Some("vs_main"),
                        buffers: &[],
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module,
                        entry_point: Some("fs_main"),
                        targets: &[Some(wgpu::ColorTargetState {
                            format,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        ..Default::default()
                    },
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState {
                        count: 1,
                        mask: !0,
                        alpha_to_coverage_enabled: false,
                    },
                    multiview_mask: None,
                    cache,
                })
            };
        let downsample_pipeline =
            make_dual_pipeline("telar-blur-downsample-pipeline", &downsample_shader);
        let upsample_pipeline =
            make_dual_pipeline("telar-blur-upsample-pipeline", &upsample_shader);

        Self {
            pipeline,
            downsample_pipeline,
            upsample_pipeline,
            sampler,
            bind_group_layout,
            format,
            use_immediates,
            intermediate_pool: Vec::new(),
        }
    }

    pub(crate) fn apply(
        &mut self,
        device: &Device,
        encoder: &mut wgpu::CommandEncoder,
        src_view: &wgpu::TextureView,
        width: u32,
        height: u32,
        sigma: f32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        if sigma > DUAL_FILTER_SIGMA_THRESHOLD {
            return self.apply_dual_filter(device, encoder, src_view, width, height, sigma);
        }
        self.apply_gaussian(device, encoder, src_view, width, height, sigma)
    }
}
