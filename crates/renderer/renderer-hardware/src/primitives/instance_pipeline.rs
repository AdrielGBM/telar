pub(crate) struct InstancePipeline<I: bytemuck::Pod> {
    pub(crate) instances_buffer: wgpu::Buffer,
    pub(crate) instances_bind_group: wgpu::BindGroup,
    pub(crate) instances_bind_group_layout: wgpu::BindGroupLayout,
    instances_capacity: usize,
    _marker: std::marker::PhantomData<I>,
}

impl<I: bytemuck::Pod> InstancePipeline<I> {
    pub(crate) fn new(device: &wgpu::Device, label: &str, initial_capacity: usize) -> Self {
        let instances_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("telar-{label}-instances")),
            size: (std::mem::size_of::<I>() * initial_capacity) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let instances_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(&format!("telar-{label}-instances-bgl")),
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
        let instances_bind_group = Self::make_instances_bind_group(
            device,
            &instances_bind_group_layout,
            &instances_buffer,
        );
        Self {
            instances_buffer,
            instances_bind_group,
            instances_bind_group_layout,
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
            &self.instances_bind_group_layout,
            &self.instances_buffer,
        );
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
