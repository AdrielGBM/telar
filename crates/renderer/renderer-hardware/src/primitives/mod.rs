pub(crate) mod image;
pub(crate) const MSAA_SAMPLES: u32 = 4;
pub(crate) mod layer;
pub(crate) mod line;
pub(crate) mod path;
pub(crate) mod rect;
pub(crate) mod text;

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

pub(super) fn encode_fill_style(fill: &renderer_core::FillStyle, tx: f32, ty: f32) -> EncodedFill {
    match fill {
        renderer_core::FillStyle::Solid(c) => EncodedFill {
            fill_type: 0,
            fill_color: c.to_array(),
            grad_p0: [0.0; 2],
            grad_p1: [0.0; 2],
            grad_radius: 0.0,
            grad_stop_count: 0,
            grad_positions: [0.0; 4],
            grad_colors: [[0.0; 4]; 4],
        },
        renderer_core::FillStyle::LinearGradient(g) => {
            let mut positions = [0.0f32; 4];
            let mut colors = [[0.0f32; 4]; 4];
            for i in 0..g.stop_count as usize {
                positions[i] = g.stops[i].position;
                colors[i] = g.stops[i].color.to_array();
            }
            EncodedFill {
                fill_type: 1,
                fill_color: [0.0; 4],
                grad_p0: [g.start.x + tx, g.start.y + ty],
                grad_p1: [g.end.x + tx, g.end.y + ty],
                grad_radius: 0.0,
                grad_stop_count: g.stop_count as u32,
                grad_positions: positions,
                grad_colors: colors,
            }
        }
        renderer_core::FillStyle::RadialGradient(g) => {
            let mut positions = [0.0f32; 4];
            let mut colors = [[0.0f32; 4]; 4];
            for i in 0..g.stop_count as usize {
                positions[i] = g.stops[i].position;
                colors[i] = g.stops[i].color.to_array();
            }
            EncodedFill {
                fill_type: 2,
                fill_color: [0.0; 4],
                grad_p0: [g.center.x + tx, g.center.y + ty],
                grad_p1: [0.0; 2],
                grad_radius: g.radius,
                grad_stop_count: g.stop_count as u32,
                grad_positions: positions,
                grad_colors: colors,
            }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Viewport {
    pub size: [f32; 2],
    pub _pad: [f32; 2],
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
                min_binding_size: None,
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
                    min_binding_size: None,
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
