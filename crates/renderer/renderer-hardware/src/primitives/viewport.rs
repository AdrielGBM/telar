use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Viewport {
    pub size: [f32; 2],
    pub offset: [f32; 2],
    pub scale: f32,
    // Pads to 16 bytes so clip_rect (a vec4 with 16-byte alignment in std140 uniform layout) starts at offset 32, matching the WGSL struct.
    pub _pad: [f32; 3],
    // Active rounded-clip rect in logical/world space; [0,0,0,0] disables the SDF clip.
    pub clip_rect: [f32; 4],
    pub clip_radius: f32,
    pub _clip_pad: [f32; 3],
}

impl Viewport {
    pub(crate) fn new(size: [f32; 2], offset: [f32; 2], scale: f32) -> Self {
        Self {
            size,
            offset,
            scale,
            _pad: [0.0; 3],
            clip_rect: [0.0; 4],
            clip_radius: 0.0,
            _clip_pad: [0.0; 3],
        }
    }
}

// Locks the GPU/CPU uniform contract: the WGSL Viewport places clip_rect at offset 32 (vec4 std140 alignment) and is 64 bytes; the Rust struct must match byte-for-byte.
const _: () = {
    assert!(std::mem::size_of::<Viewport>() == 64);
    assert!(std::mem::offset_of!(Viewport, clip_rect) == 32);
    assert!(std::mem::offset_of!(Viewport, clip_radius) == 48);
};

pub(crate) fn create_viewport_buffer(
    device: &wgpu::Device,
    label: &str,
    viewport: &Viewport,
) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::bytes_of(viewport),
        usage: wgpu::BufferUsages::UNIFORM,
    })
}

pub(crate) fn create_viewport_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("rsx-viewport-bgl"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<Viewport>() as u64),
            },
            count: None,
        }],
    })
}
