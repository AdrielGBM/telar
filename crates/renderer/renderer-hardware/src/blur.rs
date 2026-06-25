use wgpu::{Device, TextureFormat};

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
    // Kawase dual-filter pipelines, used for sigma > DUAL_FILTER_SIGMA_THRESHOLD to cut GPU bandwidth versus the separable Gaussian.
    downsample_pipeline: wgpu::RenderPipeline,
    upsample_pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    bind_group_layout: wgpu::BindGroupLayout,
    format: TextureFormat,
    use_immediates: bool,
    // Pool of intermediate textures keyed by (width, height). The intermediate is rewritten every horizontal pass (loaded with Clear), so leftover contents from a previous use are harmless.
    intermediate_pool: Vec<(wgpu::Texture, wgpu::TextureView, u32, u32)>,
}

// Above this sigma the separable Gaussian samples too many texels at full resolution; switch to the Kawase dual-filter chain instead.
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
                include_str!("blur_immediates.wgsl"),
                include_str!("blur_downsample_immediates.wgsl"),
                include_str!("blur_upsample_immediates.wgsl"),
            )
        } else {
            (
                include_str!("blur.wgsl"),
                include_str!("blur_downsample.wgsl"),
                include_str!("blur_upsample.wgsl"),
            )
        };
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rsx-blur-shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });
        let downsample_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rsx-blur-downsample-shader"),
            source: wgpu::ShaderSource::Wgsl(downsample_source.into()),
        });
        let upsample_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rsx-blur-upsample-shader"),
            source: wgpu::ShaderSource::Wgsl(upsample_source.into()),
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("rsx-blur-sampler"),
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
            label: Some("rsx-blur-bgl"),
            entries: &bind_group_layout_entries,
        });

        let immediate_size = if use_immediates {
            std::mem::size_of::<BlurParams>() as u32
        } else {
            0
        };
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rsx-blur-pipeline-layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rsx-blur-pipeline"),
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
            make_dual_pipeline("rsx-blur-downsample-pipeline", &downsample_shader);
        let upsample_pipeline = make_dual_pipeline("rsx-blur-upsample-pipeline", &upsample_shader);

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

    fn apply_gaussian(
        &mut self,
        device: &Device,
        encoder: &mut wgpu::CommandEncoder,
        src_view: &wgpu::TextureView,
        width: u32,
        height: u32,
        sigma: f32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture_usage =
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;

        // Extract field references before the closure so rustc counts them as reads.
        let pipeline = &self.pipeline;
        let bind_group_layout = &self.bind_group_layout;
        let sampler = &self.sampler;
        let use_immediates = self.use_immediates;

        let (intermediate, intermediate_view) = if let Some(pos) = self
            .intermediate_pool
            .iter()
            .position(|(_, _, w, h)| *w == width && *h == height)
        {
            let (t, v, _, _) = self.intermediate_pool.remove(pos);
            (t, v)
        } else {
            let t = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("rsx-blur-intermediate"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.format,
                usage: texture_usage,
                view_formats: &[],
            });
            let v = t.create_view(&wgpu::TextureViewDescriptor::default());
            (t, v)
        };

        let output = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rsx-blur-output"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: texture_usage,
            view_formats: &[],
        });
        let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());

        let texture_size = [width as f32, height as f32];

        let make_bg_with_params = |view: &wgpu::TextureView, params: &BlurParams| {
            if use_immediates {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("rsx-blur-bind-group"),
                    layout: bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(sampler),
                        },
                    ],
                })
            } else {
                use wgpu::util::DeviceExt;
                let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("rsx-blur-params"),
                    contents: bytemuck::bytes_of(params),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("rsx-blur-bind-group"),
                    layout: bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: params_buf.as_entire_binding(),
                        },
                    ],
                })
            }
        };

        {
            let params = BlurParams {
                direction: [1.0, 0.0],
                texture_size,
                sigma,
                _pad: [0.0; 3],
            };
            let bg = make_bg_with_params(src_view, &params);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rsx-blur-h-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &intermediate_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bg, &[]);
            if use_immediates {
                pass.set_immediates(0, bytemuck::bytes_of(&params));
            }
            pass.draw(0..6, 0..1);
        }

        {
            let params = BlurParams {
                direction: [0.0, 1.0],
                texture_size,
                sigma,
                _pad: [0.0; 3],
            };
            let bg = make_bg_with_params(&intermediate_view, &params);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rsx-blur-v-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &output_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bg, &[]);
            if use_immediates {
                pass.set_immediates(0, bytemuck::bytes_of(&params));
            }
            pass.draw(0..6, 0..1);
        }

        // Return the intermediate texture to the pool for reuse on the next blur call. The horizontal pass uses Clear on load, so any stale contents from a previous call are overwritten.
        self.intermediate_pool
            .push((intermediate, intermediate_view, width, height));

        (output, output_view)
    }

    // Kawase dual-filter blur: alternate down-sample and up-sample passes across progressively smaller textures, matching the Gaussian's perceptual radius with far fewer full-resolution fetches.
    fn apply_dual_filter(
        &mut self,
        device: &Device,
        encoder: &mut wgpu::CommandEncoder,
        src_view: &wgpu::TextureView,
        width: u32,
        height: u32,
        sigma: f32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let iterations = ((sigma / 4.0).ceil() as u32).clamp(1, 4);

        // Clone the Arc-backed GPU handles so the down/up-sample loops can keep using them while take_pooled_texture borrows &mut self.intermediate_pool.
        let downsample_pipeline = self.downsample_pipeline.clone();
        let upsample_pipeline = self.upsample_pipeline.clone();
        let bind_group_layout = self.bind_group_layout.clone();
        let sampler = self.sampler.clone();
        let use_immediates = self.use_immediates;

        // Down-sample chain. Each entry owns the texture written at that level plus the (w, h) of the level it was produced *from* (its source), needed to size up-sample outputs symmetrically.
        let mut down_levels: Vec<(wgpu::Texture, wgpu::TextureView, u32, u32)> =
            Vec::with_capacity(iterations as usize);
        let mut src_w = width;
        let mut src_h = height;

        for i in 0..iterations {
            let dst_w = (src_w / 2).max(1);
            let dst_h = (src_h / 2).max(1);
            let (tex, view) = self.take_pooled_texture(device, dst_w, dst_h);
            {
                let source_view: &wgpu::TextureView = if i == 0 {
                    src_view
                } else {
                    &down_levels[(i - 1) as usize].1
                };
                let params = BlurParams {
                    direction: [i as f32, 0.0],
                    texture_size: [src_w as f32, src_h as f32],
                    sigma: 0.0,
                    _pad: [0.0; 3],
                };
                Self::render_dual_pass(
                    device,
                    encoder,
                    &downsample_pipeline,
                    &bind_group_layout,
                    &sampler,
                    use_immediates,
                    source_view,
                    &view,
                    &params,
                    "rsx-blur-downsample-pass",
                );
            }
            // Store the source dimensions so the matching up-sample pass can recreate the symmetric output size.
            down_levels.push((tex, view, src_w, src_h));
            src_w = dst_w;
            src_h = dst_h;
        }

        // Up-sample chain. `current` is the smallest level produced above; each pass reads it and writes to the next-larger level's source dimensions until reaching full resolution.
        let mut current_w = src_w;
        let mut current_h = src_h;
        let mut current_view = down_levels.last().map(|l| l.1.clone());

        // Hold up-sample intermediates here until the whole chain finishes, so a texture still being read as a source can never be re-acquired from the pool as the next pass's write target.
        let mut up_intermediates: Vec<(wgpu::Texture, wgpu::TextureView)> = Vec::new();
        let mut output: Option<(wgpu::Texture, wgpu::TextureView)> = None;
        for i in (0..iterations).rev() {
            let (out_w, out_h) = (down_levels[i as usize].2, down_levels[i as usize].3);
            let (out_tex, out_view) = self.take_pooled_texture(device, out_w, out_h);
            {
                let source_view = current_view.as_ref().expect("up-sample source view");
                let params = BlurParams {
                    direction: [0.0, 0.0],
                    texture_size: [current_w as f32, current_h as f32],
                    sigma: 0.0,
                    _pad: [0.0; 3],
                };
                Self::render_dual_pass(
                    device,
                    encoder,
                    &upsample_pipeline,
                    &bind_group_layout,
                    &sampler,
                    use_immediates,
                    source_view,
                    &out_view,
                    &params,
                    "rsx-blur-upsample-pass",
                );
            }
            current_view = Some(out_view.clone());
            current_w = out_w;
            current_h = out_h;
            // The final iteration (i == 0) lands at full resolution; keep that texture as the result and pool everything else.
            if i == 0 {
                output = Some((out_tex, out_view));
            } else {
                up_intermediates.push((out_tex, out_view));
            }
        }

        // Return all scratch textures (down-sample levels and intermediate up-sample targets) to the pool for reuse on the next call, now that nothing reads them.
        for (tex, view, _, _) in down_levels {
            let (w, h) = (tex.size().width, tex.size().height);
            self.intermediate_pool.push((tex, view, w, h));
        }
        for (tex, view) in up_intermediates {
            let (w, h) = (tex.size().width, tex.size().height);
            self.intermediate_pool.push((tex, view, w, h));
        }

        output.expect("dual-filter always runs at least one iteration")
    }

    // Acquire a (width, height) texture from the pool or create a fresh one. Callers always render with LoadOp::Clear, so stale pooled contents are harmless.
    fn take_pooled_texture(
        &mut self,
        device: &Device,
        width: u32,
        height: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture_usage =
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        if let Some(pos) = self
            .intermediate_pool
            .iter()
            .position(|(_, _, w, h)| *w == width && *h == height)
        {
            let (t, v, _, _) = self.intermediate_pool.remove(pos);
            return (t, v);
        }
        let t = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rsx-blur-mip"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: texture_usage,
            view_formats: &[],
        });
        let v = t.create_view(&wgpu::TextureViewDescriptor::default());
        (t, v)
    }

    #[allow(clippy::too_many_arguments)]
    fn render_dual_pass(
        device: &Device,
        encoder: &mut wgpu::CommandEncoder,
        pipeline: &wgpu::RenderPipeline,
        bind_group_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        use_immediates: bool,
        src_view: &wgpu::TextureView,
        dst_view: &wgpu::TextureView,
        params: &BlurParams,
        label: &str,
    ) {
        let bg = if use_immediates {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("rsx-blur-dual-bind-group"),
                layout: bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(src_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                ],
            })
        } else {
            use wgpu::util::DeviceExt;
            let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("rsx-blur-dual-params"),
                contents: bytemuck::bytes_of(params),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("rsx-blur-dual-bind-group"),
                layout: bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(src_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: params_buf.as_entire_binding(),
                    },
                ],
            })
        };

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: dst_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bg, &[]);
        if use_immediates {
            pass.set_immediates(0, bytemuck::bytes_of(params));
        }
        pass.draw(0..6, 0..1);
    }
}
