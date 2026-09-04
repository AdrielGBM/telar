//! Render-pipeline construction shared by every primitive.

pub(crate) fn create_render_pipeline(
    device: &wgpu::Device,
    label: &str,
    shader_source: &str,
    bind_group_layouts: &[&wgpu::BindGroupLayout],
    vertex_buffers: &[Option<wgpu::VertexBufferLayout<'_>>],
    surface_format: wgpu::TextureFormat,
    msaa_samples: u32,
    cache: Option<&wgpu::PipelineCache>,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(&format!("telar-{label}-shader")),
        source: wgpu::ShaderSource::Wgsl(shader_source.to_owned().into()),
    });

    let bgl_opts: Vec<Option<&wgpu::BindGroupLayout>> =
        bind_group_layouts.iter().map(|b| Some(*b)).collect();

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(&format!("telar-{label}-pipeline-layout")),
        bind_group_layouts: &bgl_opts,
        immediate_size: 0,
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(&format!("telar-{label}-pipeline")),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: vertex_buffers,
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_format,
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
    })
}
