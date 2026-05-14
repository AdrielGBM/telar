pub(crate) mod image;
pub(crate) mod rect;
pub(crate) mod text;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Viewport {
    pub size: [f32; 2],
    pub _pad: [f32; 2],
}

pub(crate) struct InstancePipeline<I: bytemuck::Pod> {
    pub(crate) viewport_buffer: wgpu::Buffer,
    pub(crate) instances_buffer: wgpu::Buffer,
    pub(crate) instances_bind_group: wgpu::BindGroup,
    pub(crate) instances_bgl: wgpu::BindGroupLayout,
    instances_capacity: usize,
    _marker: std::marker::PhantomData<I>,
}

impl<I: bytemuck::Pod> InstancePipeline<I> {
    pub(crate) fn new(device: &wgpu::Device, label: &str, initial_capacity: usize) -> Self {
        let viewport_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("rsx-{label}-viewport")),
            size: std::mem::size_of::<Viewport>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let instances_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("rsx-{label}-instances")),
            size: (std::mem::size_of::<I>() * initial_capacity) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let instances_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(&format!("rsx-{label}-instances-bgl")),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let instances_bind_group = Self::make_instances_bind_group(
            device,
            &instances_bgl,
            &viewport_buffer,
            &instances_buffer,
        );
        Self {
            viewport_buffer,
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
        self.instances_bind_group = Self::make_instances_bind_group(
            device,
            &self.instances_bgl,
            &self.viewport_buffer,
            &self.instances_buffer,
        );
        self.instances_capacity = new_capacity;
    }

    fn make_instances_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        viewport_buffer: &wgpu::Buffer,
        instances_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: viewport_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: instances_buffer.as_entire_binding(),
                },
            ],
        })
    }
}
