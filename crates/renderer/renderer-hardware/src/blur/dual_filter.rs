use wgpu::Device;

use super::{BlurParams, BlurPipeline};

impl BlurPipeline {
    // Kawase dual-filter blur: alternate down-sample and up-sample passes across progressively smaller textures, matching the Gaussian's perceptual radius with far fewer full-resolution fetches.
    pub(super) fn apply_dual_filter(
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
                    "telar-blur-downsample-pass",
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
                    "telar-blur-upsample-pass",
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
            label: Some("telar-blur-mip"),
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
                label: Some("telar-blur-dual-bind-group"),
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
                label: Some("telar-blur-dual-params"),
                contents: bytemuck::bytes_of(params),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("telar-blur-dual-bind-group"),
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

        let mut pass =
            crate::pass::color_pass(encoder, label, dst_view, None, crate::pass::clear_store());
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bg, &[]);
        if use_immediates {
            pass.set_immediates(0, bytemuck::bytes_of(params));
        }
        pass.draw(0..6, 0..1);
    }
}
