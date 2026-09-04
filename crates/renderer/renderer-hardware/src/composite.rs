//! Compositing a rendered layer back onto its parent, with its opacity and rounded-clip mask.

use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(bytemuck::Pod, bytemuck::Zeroable, Clone, Copy)]
struct CompositeParamsRaw {
    rect: [f32; 4],
    alpha: f32,
    clip_radius: f32,
    content_uv_scale: [f32; 2],
}

pub(crate) struct CompositePipeline {
    pub(crate) pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    pub(crate) bind_group_layout: wgpu::BindGroupLayout,
    // Popped by `create_bind_group`, which pushes the used buffer into `params_buffer_in_use`.
    params_buffer_pool: Vec<wgpu::Buffer>,
    // Kept alive until the frame's GPU work is submitted, then recycled at the next `begin_frame`.
    params_buffer_in_use: Vec<wgpu::Buffer>,
}

impl CompositePipeline {
    pub(crate) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        msaa_samples: u32,
        viewport_bgl: &wgpu::BindGroupLayout,
        cache: Option<&wgpu::PipelineCache>,
    ) -> Self {
        let shader_source = include_str!("composite.wgsl");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("telar-composite-shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("telar-composite-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("telar-composite-bgl"),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<
                            CompositeParamsRaw,
                        >() as u64),
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("telar-composite-pipeline-layout"),
            bind_group_layouts: &[Some(viewport_bgl), Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("telar-composite-pipeline"),
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
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
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
                count: msaa_samples,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache,
        });

        Self {
            pipeline,
            sampler,
            bind_group_layout,
            params_buffer_pool: Vec::new(),
            params_buffer_in_use: Vec::new(),
        }
    }

    // Must run once per frame before any `create_bind_group`, and after the previous frame's submit, so the buffers are no longer referenced by in-flight GPU work.
    pub(crate) fn recycle_params_buffers(&mut self) {
        self.params_buffer_pool
            .append(&mut self.params_buffer_in_use);
    }

    pub(crate) fn create_bind_group(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        rect: [f32; 4],
        alpha: f32,
        clip_radius: f32,
        content_uv_scale: [f32; 2],
    ) -> wgpu::BindGroup {
        let params = CompositeParamsRaw {
            rect,
            alpha,
            clip_radius,
            content_uv_scale,
        };
        // Reuses a pooled buffer, writing params in place, or creates one on a miss. COPY_DST is required for `queue.write_buffer`.
        let params_buf = match self.params_buffer_pool.pop() {
            Some(buf) => {
                queue.write_buffer(&buf, 0, bytemuck::bytes_of(&params));
                buf
            }
            None => device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("telar-composite-params"),
                contents: bytemuck::bytes_of(&params),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }),
        };
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("telar-composite-bind-group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: params_buf.as_entire_binding(),
                },
            ],
        });
        // Retained for the frame; returned to the pool by `recycle_params_buffers` next frame.
        self.params_buffer_in_use.push(params_buf);
        bind_group
    }
}
