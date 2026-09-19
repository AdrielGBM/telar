//! Compositing a rendered layer back onto its parent, with its opacity, rounded-clip mask and blend mode.

use std::collections::HashMap;

use geometry_core::Rect;
use renderer_core::BlendMode;
use wgpu::util::DeviceExt;

/// Where one composite lands and what masks it on the way.
#[derive(Clone, Copy)]
pub(crate) struct CompositeParams {
    /// Destination rect, in the logical space the bound viewport maps onto NDC.
    pub(crate) rect: [f32; 4],
    pub(crate) alpha: f32,
    /// Rounds the composite's own corners, as a radius on [`Self::rect`]; zero leaves them square.
    pub(crate) clip_radius: f32,
    /// Scales the sampled UVs, for a bucketed texture wider than the content it holds.
    pub(crate) content_uv_scale: [f32; 2],
    /// The rounded clip enclosing the composite, in the same logical space as [`Self::rect`].
    ///
    /// Travels with the composite rather than through the viewport uniform every other draw reads its clip from, because a layer blit runs in a pass of its own, where the viewport the clip was pushed onto is no longer bound.
    pub(crate) outer_clip: Option<(Rect, f32)>,
}

impl CompositeParams {
    /// A whole texture blitted opaquely into `rect`, unmasked.
    pub(crate) fn blit(rect: [f32; 4]) -> Self {
        Self {
            rect,
            alpha: 1.0,
            clip_radius: 0.0,
            content_uv_scale: [1.0, 1.0],
            outer_clip: None,
        }
    }
}

#[repr(C)]
#[derive(bytemuck::Pod, bytemuck::Zeroable, Clone, Copy)]
struct CompositeParamsRaw {
    rect: [f32; 4],
    alpha: f32,
    clip_radius: f32,
    content_uv_scale: [f32; 2],
    outer_clip_rect: [f32; 4],
    outer_clip_radius: f32,
    _pad: [f32; 3],
}

// Locks the GPU/CPU contract: the WGSL `CompositeParams` places `outer_clip_rect` at offset 32 and is 64 bytes.
const _: () = {
    assert!(std::mem::size_of::<CompositeParamsRaw>() == 64);
    assert!(std::mem::offset_of!(CompositeParamsRaw, outer_clip_rect) == 32);
};

impl From<CompositeParams> for CompositeParamsRaw {
    fn from(params: CompositeParams) -> Self {
        let (outer_clip_rect, outer_clip_radius) = match params.outer_clip {
            Some((rect, radius)) => ([rect.x, rect.y, rect.width, rect.height], radius),
            None => ([0.0; 4], 0.0),
        };
        Self {
            rect: params.rect,
            alpha: params.alpha,
            clip_radius: params.clip_radius,
            content_uv_scale: params.content_uv_scale,
            outer_clip_rect,
            outer_clip_radius,
            _pad: [0.0; 3],
        }
    }
}

pub(crate) struct CompositePipeline {
    pub(crate) pipeline: wgpu::RenderPipeline,
    shader: wgpu::ShaderModule,
    sampler: wgpu::Sampler,
    pub(crate) bind_group_layout: wgpu::BindGroupLayout,
    backdrop_bind_group_layout: wgpu::BindGroupLayout,
    viewport_bind_group_layout: wgpu::BindGroupLayout,
    target: Target,
    // Built the first time a layer composites through its mode: most surfaces never blend, and each variant is a pipeline compile.
    blend_variants: HashMap<BlendMode, wgpu::RenderPipeline>,
    // Popped by `create_bind_group`, which pushes the used buffer into `params_buffer_in_use`.
    params_buffer_pool: Vec<wgpu::Buffer>,
    // Kept alive until the frame's GPU work is submitted, then recycled at the next `begin_frame`.
    params_buffer_in_use: Vec<wgpu::Buffer>,
}

#[derive(Clone)]
struct Target {
    format: wgpu::TextureFormat,
    msaa_samples: u32,
    cache: Option<wgpu::PipelineCache>,
}

/// Which modes blend in shader, reading a copy of the target, with the number `composite_blend.wgsl` knows each by. `None` for the ones fixed-function blending computes exactly on its own.
pub(crate) fn blend_shader_mode(blend: BlendMode) -> Option<u32> {
    match blend {
        BlendMode::Normal | BlendMode::Screen | BlendMode::Plus => None,
        BlendMode::Multiply => Some(1),
        BlendMode::Overlay => Some(2),
        BlendMode::Darken => Some(3),
        BlendMode::Lighten => Some(4),
        BlendMode::ColorDodge => Some(5),
        BlendMode::ColorBurn => Some(6),
        BlendMode::HardLight => Some(7),
        BlendMode::SoftLight => Some(8),
        BlendMode::Difference => Some(9),
        BlendMode::Exclusion => Some(10),
        BlendMode::Hue => Some(11),
        BlendMode::Saturation => Some(12),
        BlendMode::Color => Some(13),
        BlendMode::Luminosity => Some(14),
    }
}

/// The fixed-function state for the modes that need no copy of the target, over premultiplied colour: Screen is `s + d(1 - s)` per channel and Plus is `min(s + d, 1)`, both exact per tiny-skia's equations for them.
fn fixed_function(blend: BlendMode) -> wgpu::BlendState {
    let add = |dst_factor| wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor,
        operation: wgpu::BlendOperation::Add,
    };
    match blend {
        BlendMode::Screen => wgpu::BlendState {
            color: add(wgpu::BlendFactor::OneMinusSrc),
            alpha: add(wgpu::BlendFactor::OneMinusSrcAlpha),
        },
        BlendMode::Plus => wgpu::BlendState {
            color: add(wgpu::BlendFactor::One),
            alpha: add(wgpu::BlendFactor::One),
        },
        _ => wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING,
    }
}

impl CompositePipeline {
    pub(crate) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        msaa_samples: u32,
        viewport_bgl: &wgpu::BindGroupLayout,
        cache: Option<&wgpu::PipelineCache>,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("telar-composite-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("composite.wgsl").into()),
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

        let backdrop_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("telar-composite-backdrop-bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    },
                    count: None,
                }],
            });

        let target = Target {
            format,
            msaa_samples,
            cache: cache.cloned(),
        };
        let pipeline = build_pipeline(
            device,
            &shader,
            &[viewport_bgl, &bind_group_layout],
            &target,
            Fragment::fixed(BlendMode::Normal),
        );

        Self {
            pipeline,
            shader,
            sampler,
            bind_group_layout,
            backdrop_bind_group_layout,
            viewport_bind_group_layout: viewport_bgl.clone(),
            target,
            blend_variants: HashMap::new(),
            params_buffer_pool: Vec::new(),
            params_buffer_in_use: Vec::new(),
        }
    }

    pub(crate) fn prepare_blend(&mut self, device: &wgpu::Device, blend: BlendMode) {
        if blend == BlendMode::Normal || self.blend_variants.contains_key(&blend) {
            return;
        }
        let pipeline = match blend_shader_mode(blend) {
            None => build_pipeline(
                device,
                &self.shader,
                &[&self.viewport_bind_group_layout, &self.bind_group_layout],
                &self.target,
                Fragment::fixed(blend),
            ),
            Some(mode) => {
                let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("telar-composite-blend-shader"),
                    source: wgpu::ShaderSource::Wgsl(
                        [
                            include_str!("composite.wgsl"),
                            include_str!("composite_blend.wgsl"),
                        ]
                        .concat()
                        .into(),
                    ),
                });
                build_pipeline(
                    device,
                    &shader,
                    &[
                        &self.viewport_bind_group_layout,
                        &self.bind_group_layout,
                        &self.backdrop_bind_group_layout,
                    ],
                    &self.target,
                    Fragment {
                        entry_point: "fs_blend",
                        blend: wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING,
                        mode: Some(mode),
                    },
                )
            }
        };
        self.blend_variants.insert(blend, pipeline);
    }

    pub(crate) fn blend_pipeline(&self, blend: BlendMode) -> &wgpu::RenderPipeline {
        match blend {
            BlendMode::Normal => &self.pipeline,
            _ => self
                .blend_variants
                .get(&blend)
                .expect("prepare_blend runs before a blended composite"),
        }
    }

    /// Binds the copy of the target that a mode blending in shader reads the backdrop from, at group 2.
    pub(crate) fn backdrop_bind_group(
        &self,
        device: &wgpu::Device,
        backdrop: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("telar-composite-backdrop-bg"),
            layout: &self.backdrop_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(backdrop),
            }],
        })
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
        params: CompositeParams,
    ) -> wgpu::BindGroup {
        let params = CompositeParamsRaw::from(params);
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

struct Fragment {
    entry_point: &'static str,
    blend: wgpu::BlendState,
    mode: Option<u32>,
}

impl Fragment {
    fn fixed(blend: BlendMode) -> Self {
        Self {
            entry_point: "fs_main",
            blend: fixed_function(blend),
            mode: None,
        }
    }
}

fn build_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    bind_group_layouts: &[&wgpu::BindGroupLayout],
    target: &Target,
    fragment: Fragment,
) -> wgpu::RenderPipeline {
    let layouts: Vec<Option<&wgpu::BindGroupLayout>> = bind_group_layouts
        .iter()
        .map(|layout| Some(*layout))
        .collect();
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("telar-composite-pipeline-layout"),
        bind_group_layouts: &layouts,
        immediate_size: 0,
    });
    let constants = fragment.mode.map(|mode| [("MODE", f64::from(mode))]);
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("telar-composite-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment.entry_point),
            targets: &[Some(wgpu::ColorTargetState {
                format: target.format,
                blend: Some(fragment.blend),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: constants.as_ref().map_or(&[], |c| c.as_slice()),
                ..Default::default()
            },
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: target.msaa_samples,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview_mask: None,
        cache: target.cache.as_ref(),
    })
}
