pub(crate) mod image;
pub(crate) mod layer;
pub(crate) mod line;
pub(crate) mod path;
pub(crate) mod rect;
pub(crate) mod text;
pub(crate) mod vello_renderer;

pub(crate) fn create_render_pipeline(
    device: &wgpu::Device,
    label: &str,
    shader_source: &str,
    bind_group_layouts: &[&wgpu::BindGroupLayout],
    vertex_buffers: &[wgpu::VertexBufferLayout<'_>],
    surface_format: wgpu::TextureFormat,
    msaa_samples: u32,
    cache: Option<&wgpu::PipelineCache>,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(&format!("rsx-{label}-shader")),
        source: wgpu::ShaderSource::Wgsl(shader_source.to_owned().into()),
    });

    let bgl_opts: Vec<Option<&wgpu::BindGroupLayout>> =
        bind_group_layouts.iter().map(|b| Some(*b)).collect();

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(&format!("rsx-{label}-pipeline-layout")),
        bind_group_layouts: &bgl_opts,
        immediate_size: 0,
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(&format!("rsx-{label}-pipeline")),
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

pub(super) struct EncodedFill {
    pub fill_type: u32,
    pub fill_color: [f32; 4],
    pub grad_p0: [f32; 2],
    pub grad_p1: [f32; 2],
    pub grad_radius: f32,
    pub grad_stop_count: u32,
    pub grad_positions: [f32; 4],
    pub grad_colors: [[f32; 4]; 4],
}

pub(super) fn encode_fill_style(fill: &renderer_core::Paint, matrix: [f32; 6]) -> EncodedFill {
    let ap = |x: f32, y: f32| -> [f32; 2] {
        let [a, b, c, d, e, f] = matrix;
        [a * x + c * y + e, b * x + d * y + f]
    };
    match fill {
        renderer_core::Paint::Solid(c) => EncodedFill {
            fill_type: 0,
            fill_color: c.to_array(),
            grad_p0: [0.0; 2],
            grad_p1: [0.0; 2],
            grad_radius: 0.0,
            grad_stop_count: 0,
            grad_positions: [0.0; 4],
            grad_colors: [[0.0; 4]; 4],
        },
        renderer_core::Paint::Gradient(g) => {
            let mut positions = [0.0f32; 4];
            let mut colors = [[0.0f32; 4]; 4];
            for (i, s) in g.stops.active().iter().enumerate() {
                positions[i] = s.position;
                colors[i] = s.color.to_array();
            }
            let stop_count = g.stops.active().len() as u32;
            match g.kind {
                renderer_core::GradientKind::Linear { start, end } => EncodedFill {
                    fill_type: 1,
                    fill_color: [0.0; 4],
                    grad_p0: ap(start.x, start.y),
                    grad_p1: ap(end.x, end.y),
                    grad_radius: 0.0,
                    grad_stop_count: stop_count,
                    grad_positions: positions,
                    grad_colors: colors,
                },
                renderer_core::GradientKind::Radial { center, radius } => EncodedFill {
                    fill_type: 2,
                    fill_color: [0.0; 4],
                    grad_p0: ap(center.x, center.y),
                    grad_p1: [0.0; 2],
                    grad_radius: radius,
                    grad_stop_count: stop_count,
                    grad_positions: positions,
                    grad_colors: colors,
                },
            }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Viewport {
    pub size: [f32; 2],
    pub offset: [f32; 2],
    pub scale: f32,
    pub _pad: f32,
}

pub(crate) fn create_viewport_bgl(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("rsx-viewport-bgl"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<Viewport>() as u64),
            },
            count: None,
        }],
    })
}

pub(crate) struct InstancePipeline<I: bytemuck::Pod> {
    pub(crate) instances_buffer: wgpu::Buffer,
    pub(crate) instances_bind_group: wgpu::BindGroup,
    pub(crate) instances_bgl: wgpu::BindGroupLayout,
    instances_capacity: usize,
    _marker: std::marker::PhantomData<I>,
}

impl<I: bytemuck::Pod> InstancePipeline<I> {
    pub(crate) fn new(device: &wgpu::Device, label: &str, initial_capacity: usize) -> Self {
        let instances_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("rsx-{label}-instances")),
            size: (std::mem::size_of::<I>() * initial_capacity) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let instances_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(&format!("rsx-{label}-instances-bgl")),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<I>() as u64),
                },
                count: None,
            }],
        });
        let instances_bind_group =
            Self::make_instances_bind_group(device, &instances_bgl, &instances_buffer);
        Self {
            instances_buffer,
            instances_bind_group,
            instances_bgl,
            instances_capacity: initial_capacity,
            _marker: std::marker::PhantomData,
        }
    }

    pub(crate) fn ensure_capacity(&mut self, device: &wgpu::Device, count: usize) {
        if count <= self.instances_capacity {
            return;
        }
        let new_capacity = (count * 2).max(self.instances_capacity * 2);
        self.instances_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (std::mem::size_of::<I>() * new_capacity) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.instances_bind_group =
            Self::make_instances_bind_group(device, &self.instances_bgl, &self.instances_buffer);
        self.instances_capacity = new_capacity;
    }

    fn make_instances_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        instances_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: instances_buffer.as_entire_binding(),
            }],
        })
    }
}
